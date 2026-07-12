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

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::OpenApi;

    #[test]
    fn openapi_spec_is_valid() {
        let json_str = ApiDoc::openapi().to_json().expect("failed to serialize OpenAPI spec");
        let body: serde_json::Value =
            serde_json::from_str(&json_str).expect("OpenAPI spec is not valid JSON");

        // Verify OpenAPI version (utoipa 5.x generates 3.1.0)
        assert_eq!(body["openapi"], "3.1.0", "expected OpenAPI version 3.1.0");

        // Verify title
        assert_eq!(body["info"]["title"], "ecs-sd", "unexpected API title");

        // Verify all 8 expected endpoint paths exist
        let expected_paths = [
            "/sd",
            "/sd/refresh",
            "/health",
            "/health/live",
            "/health/ready",
            "/metrics",
            "/config",
            "/proxy/{id}/{path}",
        ];
        for path in &expected_paths {
            assert!(
                body["paths"].get(*path).is_some(),
                "missing expected path: {path}"
            );
        }

        // Verify all 10 expected schema names exist
        let expected_schemas = [
            "Target",
            "MetadataLevel",
            "FilterMode",
            "HealthResponse",
            "CacheHealth",
            "ClusterHealth",
            "LastRefreshHealth",
            "ConfigResponse",
            "Mode",
            "ClusterMode",
        ];
        let schemas = body["components"]["schemas"]
            .as_object()
            .expect("components.schemas is not an object");
        for name in &expected_schemas {
            assert!(
                schemas.contains_key(*name),
                "missing expected schema: {name}"
            );
        }
    }
}
