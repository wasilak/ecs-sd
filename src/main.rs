mod error;
mod config;
mod state;
mod aws;
mod models;
mod routes;
mod handlers;

use axum::Router;
use rand::Rng;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, SystemTime};
use tokio::signal;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;
use tracing::info;
use tracing::warn;

use crate::config::Config;
use crate::models::{MetadataLevel, Target};
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

    // Create shared state
    let state = AppState::new(
        config.clone(),
        ecs_client,
        ec2_client,
        sts_client,
        region,
    )
    .await
    .map_err(|e| {
        eprintln!("Failed to initialize discovery service: {}", e);
        std::process::exit(1);
    })?;

    // Perform initial discovery to populate all cache tiers
    info!("Performing initial discovery...");
    let targets_aws = state.discovery.discover_all_clusters(&config.clusters).await;
    replace_cache_levels_and_refresh_time(&state, targets_aws).await;

    info!("Initial discovery complete");

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let refresh_handle = spawn_background_refresh(state.clone(), shutdown_rx);

    // Build router
    let app = Router::new()
        .merge(routes::create_routes())
        .with_state(state);

    // Parse bind address
    let addr: SocketAddr = config.listen.parse()?;
    info!("Listening on {}", addr);

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

    info!("Server shut down gracefully");
    Ok(())
}

/// Filter target labels to only include those for the specified level
fn filter_labels_by_level(target: &Target, level: MetadataLevel) -> Target {
    let filtered_labels: HashMap<String, String> = target
        .labels
        .iter()
        .filter(|(key, _)| {
            // Determine which level this label belongs to based on prefix
            let label_level = if key.starts_with("__meta_ecs_container_") || *key == "__meta_ecs_metrics_port" {
                MetadataLevel::Container
            } else if key.starts_with("__meta_ecs_task_") {
                MetadataLevel::Task
            } else if key.starts_with("__meta_ecs_service_") {
                MetadataLevel::Service
            } else if key.starts_with("__meta_ecs_cluster_") {
                MetadataLevel::Cluster
            } else if key.starts_with("__meta_ecs_") {
                MetadataLevel::Aws
            } else {
                MetadataLevel::Container // Default for unknown labels
            };
            
            level.includes(label_level)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    
    Target {
        targets: target.targets.clone(),
        labels: filtered_labels,
    }
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

                    let jitter_factor = rand::thread_rng().gen_range(-0.10..=0.10);
                    let jittered_delay = calculate_jittered_delay(base_interval, jitter_factor);
                    tokio::time::sleep(jittered_delay).await;

                    if *shutdown_rx.borrow() {
                        break;
                    }

                    info!("discovery refresh started");
                    match refresh_cache_once(&state).await {
                        Ok(target_count) => {
                            info!("discovery refresh complete: {} targets", target_count);
                        }
                        Err(error) => {
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
        .discover_all_clusters(&state.config.clusters)
        .await;
    let target_count = targets_aws.len();

    replace_cache_levels_and_refresh_time(state, targets_aws).await;

    Ok(target_count)
}

async fn replace_cache_levels_and_refresh_time(state: &AppState, targets_aws: Vec<Target>) {
    let targets_cluster: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Cluster))
        .collect();
    let targets_service: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Service))
        .collect();
    let targets_task: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Task))
        .collect();
    let targets_container: Vec<Target> = targets_aws
        .iter()
        .map(|t| filter_labels_by_level(t, MetadataLevel::Container))
        .collect();

    {
        let mut cache = state.cache.write().await;
        cache.insert(MetadataLevel::Aws, targets_aws);
        cache.insert(MetadataLevel::Cluster, targets_cluster);
        cache.insert(MetadataLevel::Service, targets_service);
        cache.insert(MetadataLevel::Task, targets_task);
        cache.insert(MetadataLevel::Container, targets_container);
    }

    {
        let mut last_refresh = state.last_refresh.write().await;
        *last_refresh = SystemTime::now();
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
