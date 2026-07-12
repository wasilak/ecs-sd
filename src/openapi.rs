use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "ecs-sd",
        version = env!("CARGO_PKG_VERSION"),
        description = "ECS HTTP Service Discovery — automatic discovery of metrics endpoints"
    ),
    paths(
        crate::handlers::sd::sd_handler,
        crate::handlers::sd::refresh_handler,
        crate::handlers::health::health_handler,
        crate::handlers::health::health_live_handler,
        crate::handlers::health::health_ready_handler,
        crate::handlers::metrics::metrics_handler,
        crate::handlers::config::config_handler,
        crate::handlers::proxy::proxy_handler,
    ),
    components(schemas(
        crate::models::Target,
        crate::models::MetadataLevel,
        crate::models::FilterMode,
        crate::handlers::health::HealthResponse,
        crate::handlers::health::CacheHealth,
        crate::handlers::health::ClusterHealth,
        crate::handlers::health::LastRefreshHealth,
        crate::handlers::config::ConfigResponse,
        crate::config::Mode,
        crate::config::ClusterMode,
    )),
    tags(
        (name = "discovery", description = "Service discovery endpoints"),
        (name = "health", description = "Health and readiness probes"),
        (name = "operations", description = "Operational endpoints"),
        (name = "proxy", description = "Prometheus scrape proxy"),
    )
)]
pub struct ApiDoc;
