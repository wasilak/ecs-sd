mod error;
mod config;
mod state;
mod aws;
mod models;
mod routes;
mod handlers;

use axum::Router;
use std::net::SocketAddr;
use tokio::signal;
use tracing::info;

use crate::config::Config;
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

    // Create shared state
    let state = AppState::new(config.clone(), ecs_client, ec2_client);

    // Perform initial discovery
    info!("Performing initial discovery...");
    let targets = state.discovery.discover_all_clusters(&config.clusters).await;

    // Write to cache
    {
        let mut cache = state.cache.write().await;
        *cache = targets;
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
