mod error;
mod config;
mod state;
mod aws;
mod models;
mod routes;
mod handlers;
mod cluster;
pub mod metrics;

use axum::Router;
use axum::routing::get;
use rand::RngExt as _;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;
use tracing::info;
use tracing::warn;

use chitchat::{spawn_chitchat, ChitchatConfig, ChitchatId, FailureDetectorConfig};
use chitchat::transport::UdpTransport;
use crate::cluster::{ClusterState, GossipProxyTarget};
use crate::config::{Config, ClusterMode};
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("Starting ecs-sd server");

    // Parse startup config from CLI/env/defaults
    let config = match Config::from_process_args() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Startup configuration error: {}", error);
            std::process::exit(1);
        }
    };

    // Create AWS clients
    let (ecs_client, ec2_client) = aws::client::create_clients().await?;
    let sts_client = aws::client::create_sts_client().await;
    
    // Extract region from SDK config
    let sdk_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let region = sdk_config
        .region()
        .map(|r| r.to_string())
        .unwrap_or_else(|| "us-east-1".to_string());

    // Initialize cluster state if cluster mode is active
    let cluster: Option<std::sync::Arc<ClusterState>> = match config.cluster_mode {
        ClusterMode::Standalone => None,
        ClusterMode::Cluster => {
            let chitchat_id = ChitchatId {
                node_id: config.node_id.clone().into(),
                generation_id: rand::random::<u64>(),
                gossip_advertise_addr: format!("0.0.0.0:{}", config.gossip_port).parse()
                    .expect("gossip_port is validated at config parse time"),
            };
            let cc_config = ChitchatConfig {
                chitchat_id,
                cluster_id: "ecs-sd".to_string(),
                gossip_interval: std::time::Duration::from_secs(1),
                listen_addr: format!("0.0.0.0:{}", config.gossip_port).parse()
                    .expect("gossip_port is validated at config parse time"),
                seed_nodes: config.cluster_seeds.clone(),
                failure_detector_config: FailureDetectorConfig::default(),
                marked_for_deletion_grace_period: std::time::Duration::from_secs(3600),
                catchup_callback: None,
                extra_liveness_predicate: None,
            };
            let handle = spawn_chitchat(cc_config, vec![], &UdpTransport).await
                .map_err(|e| { eprintln!("Failed to start gossip: {}", e); std::process::exit(1) })?;
            info!("Gossip node {} started on port {}", config.node_id, config.gossip_port);
            Some(std::sync::Arc::new(ClusterState { handle, self_id: config.node_id.clone() }))
        }
    };

    // Initialize metrics state
    let metrics = Arc::new(crate::metrics::MetricsState::new()
        .expect("failed to initialize metrics"));

    // Create shared state
    let state = AppState::new(
        config.clone(),
        ecs_client,
        ec2_client,
        sts_client,
        region,
        cluster,
        metrics.clone(),
    )
    .await
    .map_err(|e| {
        eprintln!("Failed to initialize discovery service: {}", e);
        std::process::exit(1);
    })?;

    // Perform initial discovery — leader only (or standalone)
    let should_discover = match &state.cluster {
        None => true,  // standalone always discovers
        Some(c) => c.is_leader().await,
    };
    if should_discover {
        info!("Performing initial discovery...");
        match state.discovery.discover_all_clusters(&config.clusters, config.mode.clone()).await {
            Ok(targets_aws) => {
                state.replace_cache_and_routing(targets_aws).await;
                info!("Initial discovery complete");
                publish_cache_to_gossip(&state).await;
            }
            Err(e) => {
                warn!("Initial discovery failed — starting with empty cache: {}", e);
            }
        }
    } else {
        info!("Follower node: skipping initial discovery, waiting for gossip cache");
    }

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let refresh_handle = spawn_background_refresh(state.clone(), shutdown_rx.clone());
    let follower_sync_handle = state.cluster.as_ref().map(|c| {
        spawn_follower_sync(state.clone(), c.clone(), shutdown_rx.clone())
    });

    // Build router
    let app = Router::new()
        .merge(routes::create_routes())
        .with_state(state.clone());

    // Parse bind address
    let addr: SocketAddr = config.listen.parse()?;
    info!("Listening on {}", addr);

    // Optional: spawn separate metrics server if metrics_port is configured
    let _metrics_handle = if let Some(metrics_port) = config.metrics_port {
        let metrics_addr: SocketAddr = format!("0.0.0.0:{}", metrics_port).parse()?;
        info!("Metrics endpoint on {}", metrics_addr);

        let metrics_app = Router::new()
            .route("/metrics", get(crate::handlers::metrics::metrics_handler))
            .with_state(state.clone());

        let metrics_listener = tokio::net::TcpListener::bind(metrics_addr).await?;
        Some(tokio::spawn(async move {
            if let Err(e) = axum::serve(metrics_listener, metrics_app).await {
                tracing::error!("metrics server error: {}", e);
            }
        }))
    } else {
        info!("Metrics endpoint on {}/metrics", addr);
        None
    };

    // Start server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx.clone()))
        .await?;

    if !*shutdown_tx.borrow() {
        let _ = shutdown_tx.send(true);
    }

    if let Err(error) = refresh_handle.await {
        warn!("background refresh task failed to join: {}", error);
    }

    if let Some(handle) = follower_sync_handle {
        if let Err(e) = handle.await {
            warn!("follower sync task failed to join: {}", e);
        }
    }

    // Shut down gossip node
    if let Some(cluster) = state.cluster {
        if let Ok(cluster) = std::sync::Arc::try_unwrap(cluster) {
            if let Err(e) = cluster.handle.shutdown().await {
                warn!("gossip shutdown error: {}", e);
            } else {
                info!("Gossip node shut down");
            }
        }
    }

    info!("Server shut down gracefully");
    Ok(())
}

fn spawn_background_refresh(
    state: AppState,
    mut shutdown_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let base_interval = Duration::from_secs(state.config.refresh_interval.max(1));
        let mut interval = create_refresh_interval(base_interval);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }

                    // Skip refresh if this is a follower node
                    if let Some(ref cluster) = state.cluster {
                        if !cluster.is_leader().await {
                            // Follower: cache is managed by follower sync task
                            continue;
                        }
                    }

                    let jitter_factor = rand::rng().random_range(-0.10..=0.10);
                    let jittered_delay = calculate_jittered_delay(base_interval, jitter_factor);
                    tokio::time::sleep(jittered_delay).await;

                    if *shutdown_rx.borrow() {
                        break;
                    }

                    info!("discovery refresh started");
                    let timer = state.metrics.discovery_duration.start_timer();
                    match refresh_cache_once(&state).await {
                        Ok(target_count) => {
                            timer.observe_duration();
                            state.metrics.discovery_targets.set(target_count as f64);
                            state.metrics.cache_refreshes
                                .with_label_values(&["success"])
                                .inc();
                            info!("discovery refresh complete: {} targets", target_count);
                            publish_cache_to_gossip(&state).await;
                        }
                        Err(error) => {
                            timer.observe_duration();
                            state.metrics.discovery_errors.inc();
                            state.metrics.cache_refreshes
                                .with_label_values(&["error"])
                                .inc();
                            warn!("discovery refresh failed: {}", error);
                        }
                    }

                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
                changed = shutdown_rx.changed() => {
                    if changed.is_ok() && *shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }
    })
}

fn create_refresh_interval(base_interval: Duration) -> tokio::time::Interval {
    let mut interval = tokio::time::interval(base_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    interval
}

async fn refresh_cache_once(state: &AppState) -> Result<usize, String> {
    if state.config.clusters.is_empty() {
        return Err("no clusters configured for refresh".to_string());
    }

    let targets_aws = state
        .discovery
        .discover_all_clusters(&state.config.clusters, state.config.mode.clone())
        .await
        .map_err(|e| {
            warn!("All clusters failed during background refresh: {}", e);
            e.to_string()
        })?;
    let target_count = targets_aws.len();

    state.replace_cache_and_routing(targets_aws).await;

    Ok(target_count)
}

fn calculate_jittered_delay(base_interval: Duration, jitter_factor: f64) -> Duration {
    let base_ms = base_interval.as_millis() as f64;
    let jittered_ms = (base_ms + (base_ms * jitter_factor)).max(1_000.0);
    Duration::from_millis(jittered_ms.round() as u64)
}

async fn shutdown_signal(shutdown_tx: watch::Sender<bool>) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("Received Ctrl+C, shutting down"),
        _ = terminate => info!("Received SIGTERM, shutting down"),
    }

    let _ = shutdown_tx.send(true);
}

async fn publish_cache_to_gossip(state: &AppState) {
    let Some(ref cluster) = state.cluster else { return };
    let snap = state.snapshot.read().await;
    if let Some(targets) = snap.cache.get(&crate::models::MetadataLevel::Aws) {
        if let Ok(json) = serde_json::to_string(targets) {
            cluster.publish_cache(&json).await;
        }
    }
    if state.config.mode == crate::config::Mode::Proxy {
        let gossip_rt: Vec<GossipProxyTarget> = snap.routing_table.values().map(|pt| GossipProxyTarget {
            route_id: pt.route_id.to_string(),
            address: pt.address.clone(),
            labels: pt.labels.clone(),
        }).collect();
        if let Ok(json) = serde_json::to_string(&gossip_rt) {
            cluster.publish_routing(&json).await;
        }
    }
}

fn spawn_follower_sync(
    state: AppState,
    cluster: std::sync::Arc<ClusterState>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if *shutdown_rx.borrow() { break; }
                    if cluster.is_leader().await { continue; }
                    if let Some(json) = cluster.read_leader_cache().await {
                        match serde_json::from_str::<Vec<crate::models::Target>>(&json) {
                            Ok(targets) => {
                                state.replace_cache_and_routing(targets).await;
                                tracing::debug!("follower cache synced from gossip");
                            }
                            Err(e) => {
                                tracing::warn!("follower: malformed gossip cache JSON: {}", e);
                            }
                        }
                    }
                }
                changed = shutdown_rx.changed() => {
                    if changed.is_ok() && *shutdown_rx.borrow() { break; }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn require_region_errors_when_none() {
        let result = require_region(None);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("AWS region"),
            "error message must mention 'AWS region', got: {msg}"
        );
    }

    #[test]
    fn require_region_returns_region_when_present() {
        let result = require_region(Some("eu-west-1".to_string()));
        assert_eq!(result, Ok("eu-west-1".to_string()));
    }

    #[test]
    fn jittered_delay_stays_within_plus_minus_ten_percent_bounds() {
        let base = Duration::from_secs(60);
        let high = calculate_jittered_delay(base, 0.10);
        let low = calculate_jittered_delay(base, -0.10);

        assert_eq!(high.as_secs(), 66);
        assert_eq!(low.as_secs(), 54);
    }

    #[test]
    fn jittered_delay_never_drops_below_one_second() {
        let base = Duration::from_secs(1);
        let delay = calculate_jittered_delay(base, -0.90);

        assert_eq!(delay.as_secs(), 1);
    }

    #[tokio::test]
    async fn ttl_refresh_loop_uses_skip_missed_tick_behavior() {
        let interval = create_refresh_interval(Duration::from_secs(30));

        assert_eq!(interval.missed_tick_behavior(), MissedTickBehavior::Skip);
    }

    #[test]
    fn ttl_refresh_lifecycle_has_no_request_trigger_primitives() {
        let main_src = include_str!("main.rs");
        let sd_src = include_str!("handlers/sd.rs");

        let token_one = ["refresh", "trigger"].join("_");
        let token_two = ["force", "refresh", "rx"].join("_");
        let token_three = ["try", "send"].join("_");

        for token in [token_one, token_two, token_three] {
            assert!(!main_src.contains(&token), "main.rs must not contain {token}");
            assert!(!sd_src.contains(&token), "handlers/sd.rs must not contain {token}");
        }
    }
}
