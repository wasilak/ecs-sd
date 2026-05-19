mod error;
mod config;
mod state;
mod aws;
mod models;
mod routes;
mod handlers;

use axum::Router;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::signal;
use tracing::info;

use crate::config::Config;
use crate::models::{MetadataLevel, Target};
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting ecs-sd server");

    // Create config (hardcoded for Phase 1)
    let config = Config::new(vec![
        "service-platform-default".to_string(),
    ]);

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
    
    // Derive all cache tiers from Aws-level targets
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

    // Populate cache
    {
        let mut cache = state.cache.write().await;
        cache.insert(MetadataLevel::Aws, targets_aws);
        cache.insert(MetadataLevel::Cluster, targets_cluster);
        cache.insert(MetadataLevel::Service, targets_service);
        cache.insert(MetadataLevel::Task, targets_task);
        cache.insert(MetadataLevel::Container, targets_container);
    }

    info!("Initial discovery complete");

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
        .with_graceful_shutdown(shutdown_signal())
        .await?;

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

async fn shutdown_signal() {
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
}
