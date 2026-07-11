use super::*;

#[test]
fn metrics_state_new_succeeds() {
    let metrics = MetricsState::new();
    assert!(metrics.is_ok(), "MetricsState::new() should succeed");
}

#[test]
fn metrics_state_has_registry() {
    let metrics = MetricsState::new().unwrap();
    // Registry exists implicitly via gather()
    let families = metrics.registry.gather();
    // Should have at least 7 metric families (CounterVec metrics appear after first use)
    assert!(families.len() >= 7, "should have at least 7 registered metric families, got {}", families.len());
}

#[test]
fn discovery_duration_histogram_exists() {
    let metrics = MetricsState::new().unwrap();
    metrics.discovery_duration.observe(0.5);
    // Verify it was recorded by checking gather output
    let families = metrics.registry.gather();
    let found = families.iter().any(|f| f.name() == "ecs_sd_discovery_duration_seconds");
    assert!(found, "discovery_duration metric should exist");
}

#[test]
fn discovery_targets_gauge_works() {
    let metrics = MetricsState::new().unwrap();
    metrics.discovery_targets.set(42.0);
    let families = metrics.registry.gather();
    let found = families.iter().any(|f| f.name() == "ecs_sd_discovery_targets_total");
    assert!(found, "discovery_targets metric should exist");
}

#[test]
fn discovery_errors_counter_works() {
    let metrics = MetricsState::new().unwrap();
    metrics.discovery_errors.inc();
    metrics.discovery_errors.inc();
    let families = metrics.registry.gather();
    let found = families.iter().any(|f| f.name() == "ecs_sd_discovery_errors_total");
    assert!(found, "discovery_errors metric should exist");
}

#[test]
fn cache_refreshes_countervec_works() {
    let metrics = MetricsState::new().unwrap();
    metrics.cache_refreshes.with_label_values(&["success"]).inc();
    metrics.cache_refreshes.with_label_values(&["success"]).inc();
    metrics.cache_refreshes.with_label_values(&["error"]).inc();
    let families = metrics.registry.gather();
    let found = families.iter().any(|f| f.name() == "ecs_sd_cache_refreshes_total");
    assert!(found, "cache_refreshes metric should exist");
    // Should have 2 metrics: one for success, one for error
    let family = families.iter().find(|f| f.name() == "ecs_sd_cache_refreshes_total").unwrap();
    assert_eq!(family.get_metric().len(), 2, "should have metrics for both labels");
}

#[test]
fn proxy_duration_histogram_works() {
    let metrics = MetricsState::new().unwrap();
    metrics.proxy_duration.observe(0.1);
    let families = metrics.registry.gather();
    let found = families.iter().any(|f| f.name() == "ecs_sd_proxy_duration_seconds");
    assert!(found, "proxy_duration should exist");
}

#[test]
fn proxy_requests_countervec_works() {
    let metrics = MetricsState::new().unwrap();
    metrics.proxy_requests.with_label_values(&["200"]).inc();
    metrics.proxy_requests.with_label_values(&["500"]).inc();
    let families = metrics.registry.gather();
    let found = families.iter().any(|f| f.name() == "ecs_sd_proxy_requests_total");
    assert!(found, "proxy_requests metric should exist");
    let family = families.iter().find(|f| f.name() == "ecs_sd_proxy_requests_total").unwrap();
    assert_eq!(family.get_metric().len(), 2, "should have metrics for both status codes");
}

#[test]
fn cluster_is_leader_gauge_works() {
    let metrics = MetricsState::new().unwrap();
    metrics.cluster_is_leader.set(1.0);
    let families = metrics.registry.gather();
    let found = families.iter().any(|f| f.name() == "ecs_sd_cluster_is_leader");
    assert!(found, "cluster_is_leader metric should exist");
}

#[test]
fn new_metric_families_are_registered_after_first_use() {
    let metrics = MetricsState::new().unwrap();

    metrics
        .http_requests_total
        .with_label_values(&["/sd", "GET", "200"])
        .inc();
    metrics
        .http_request_duration_seconds
        .with_label_values(&["/sd", "GET"])
        .observe(0.005);
    metrics
        .discovery_targets_per_cluster
        .with_label_values(&["prod"])
        .set(3.0);
    metrics
        .discovery_target_churn_total
        .with_label_values(&["added"])
        .inc();
    metrics
        .aws_api_calls_total
        .with_label_values(&["list_tasks"])
        .inc();
    metrics
        .cache_follower_syncs_total
        .with_label_values(&["success"])
        .inc();
    metrics.startup_duration_seconds.set(1.5);

    let families = metrics.registry.gather();
    for name in [
        "ecs_sd_http_requests_total",
        "ecs_sd_http_request_duration_seconds",
        "ecs_sd_discovery_targets_per_cluster",
        "ecs_sd_discovery_target_churn_total",
        "ecs_sd_aws_api_calls_total",
        "ecs_sd_cache_follower_syncs_total",
        "ecs_sd_startup_duration_seconds",
    ] {
        assert!(
            families.iter().any(|f| f.name() == name),
            "{name} metric should exist"
        );
    }
}
