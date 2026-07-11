use prometheus::{Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec, Opts, Registry};

/// Owns the Prometheus Registry and all metric instances.
/// Stored in AppState; metrics are registered once at startup.
pub struct MetricsState {
    pub registry: Registry,
    // Discovery metrics (MET-02)
    pub discovery_duration: Histogram,
    pub discovery_targets: Gauge,
    pub discovery_errors: Counter,
    // Cache metrics (MET-03)
    pub cache_age: Gauge,
    pub cache_refreshes: CounterVec,
    // Proxy metrics (MET-04)
    pub proxy_requests: CounterVec,
    pub proxy_duration: Histogram,
    // Cluster metrics (MET-05)
    pub cluster_nodes: Gauge,
    pub cluster_is_leader: Gauge,
    // New metric families (MET-08..MET-14)
    pub http_requests_total: CounterVec,
    pub http_request_duration_seconds: HistogramVec,
    pub discovery_targets_per_cluster: GaugeVec,
    pub discovery_target_churn_total: CounterVec,
    pub aws_api_calls_total: CounterVec,
    pub cache_follower_syncs_total: CounterVec,
    pub startup_duration_seconds: Gauge,
}

impl MetricsState {
    /// Create a new MetricsState with all metrics registered.
    /// Returns Err if any metric registration fails (should not happen with unique names).
    pub fn new() -> prometheus::Result<Self> {
        let registry = Registry::new();

        // Discovery metrics (MET-02)
        let discovery_duration = Histogram::with_opts(
            HistogramOpts::new(
                "ecs_sd_discovery_duration_seconds",
                "Discovery duration in seconds"
            )
            .buckets(prometheus::exponential_buckets(0.01, 2.0, 16).unwrap())
        )?;
        let discovery_targets = Gauge::new(
            "ecs_sd_discovery_targets_total",
            "Total number of discovered targets"
        )?;
        let discovery_errors = Counter::new(
            "ecs_sd_discovery_errors_total",
            "Total number of discovery errors"
        )?;

        // Cache metrics (MET-03)
        let cache_age = Gauge::new(
            "ecs_sd_cache_age_seconds",
            "Age of cache in seconds since last refresh"
        )?;
        let cache_refreshes = CounterVec::new(
            Opts::new(
                "ecs_sd_cache_refreshes_total",
                "Total number of cache refreshes"
            ),
            &["result"]
        )?;

        // Proxy metrics (MET-04)
        let proxy_requests = CounterVec::new(
            Opts::new(
                "ecs_sd_proxy_requests_total",
                "Total number of proxy requests"
            ),
            &["status"]
        )?;
        let proxy_duration = Histogram::with_opts(
            HistogramOpts::new(
                "ecs_sd_proxy_duration_seconds",
                "Proxy request duration in seconds"
            )
            .buckets(prometheus::exponential_buckets(0.001, 2.0, 15).unwrap())
        )?;

        // Cluster metrics (MET-05)
        let cluster_nodes = Gauge::new(
            "ecs_sd_cluster_nodes_total",
            "Total number of nodes in the cluster"
        )?;
        let cluster_is_leader = Gauge::new(
            "ecs_sd_cluster_is_leader",
            "Whether this node is the leader (1=yes, 0=no)"
        )?;

        // New metric families (MET-08..MET-14)
        let http_requests_total = CounterVec::new(
            Opts::new("ecs_sd_http_requests_total", "Total HTTP requests"),
            &["endpoint", "method", "status_code"]
        )?;
        let http_request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "ecs_sd_http_request_duration_seconds",
                "HTTP request duration in seconds"
            )
            .buckets(prometheus::exponential_buckets(0.001, 2.0, 14).unwrap()),
            &["endpoint", "method"]
        )?;
        let discovery_targets_per_cluster = GaugeVec::new(
            Opts::new(
                "ecs_sd_discovery_targets_per_cluster",
                "Targets per ECS cluster"
            ),
            &["cluster"]
        )?;
        let discovery_target_churn_total = CounterVec::new(
            Opts::new(
                "ecs_sd_discovery_target_churn_total",
                "Target churn since last refresh"
            ),
            &["change"]
        )?;
        let aws_api_calls_total = CounterVec::new(
            Opts::new("ecs_sd_aws_api_calls_total", "AWS SDK API calls"),
            &["operation"]
        )?;
        let cache_follower_syncs_total = CounterVec::new(
            Opts::new(
                "ecs_sd_cache_follower_syncs_total",
                "Follower cache sync outcomes"
            ),
            &["result"]
        )?;
        let startup_duration_seconds = Gauge::new(
            "ecs_sd_startup_duration_seconds",
            "Seconds from process start to first successful cache population"
        )?;

        // Register all metrics
        registry.register(Box::new(discovery_duration.clone()))?;
        registry.register(Box::new(discovery_targets.clone()))?;
        registry.register(Box::new(discovery_errors.clone()))?;
        registry.register(Box::new(cache_age.clone()))?;
        registry.register(Box::new(cache_refreshes.clone()))?;
        registry.register(Box::new(proxy_requests.clone()))?;
        registry.register(Box::new(proxy_duration.clone()))?;
        registry.register(Box::new(cluster_nodes.clone()))?;
        registry.register(Box::new(cluster_is_leader.clone()))?;
        registry.register(Box::new(http_requests_total.clone()))?;
        registry.register(Box::new(http_request_duration_seconds.clone()))?;
        registry.register(Box::new(discovery_targets_per_cluster.clone()))?;
        registry.register(Box::new(discovery_target_churn_total.clone()))?;
        registry.register(Box::new(aws_api_calls_total.clone()))?;
        registry.register(Box::new(cache_follower_syncs_total.clone()))?;
        registry.register(Box::new(startup_duration_seconds.clone()))?;

        Ok(Self {
            registry,
            discovery_duration,
            discovery_targets,
            discovery_errors,
            cache_age,
            cache_refreshes,
            proxy_requests,
            proxy_duration,
            cluster_nodes,
            cluster_is_leader,
            http_requests_total,
            http_request_duration_seconds,
            discovery_targets_per_cluster,
            discovery_target_churn_total,
            aws_api_calls_total,
            cache_follower_syncs_total,
            startup_duration_seconds,
        })
    }
}

#[cfg(test)]
mod tests;
