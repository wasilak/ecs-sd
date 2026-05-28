use std::ffi::OsString;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use clap::Parser;

use crate::error::ConfigError;
use crate::models::MetadataLevel;

#[derive(Debug, Clone, PartialEq, Default, clap::ValueEnum)]
pub enum ClusterMode {
    #[default]
    Standalone,
    Cluster,
}

#[derive(Debug, Clone, PartialEq, Default, clap::ValueEnum)]
pub enum Mode {
    #[default]
    Discovery,
    Proxy,
}

#[derive(Parser, Debug, Clone)]
#[command(name = "ecs-sd", about = "ECS Prometheus Service Discovery")]
pub struct Args {
    #[arg(
        long,
        env = "ECS_SD_CLUSTERS",
        required = true,
        help = "Comma-separated list of ECS cluster names or ARNs"
    )]
    pub clusters: String,

    #[arg(
        long,
        env = "ECS_SD_LISTEN",
        default_value = "0.0.0.0:8080",
        help = "Socket address to bind (host:port)"
    )]
    pub listen: String,

    #[arg(
        long,
        env = "ECS_SD_REFRESH_INTERVAL",
        default_value = "60s",
        value_parser = humantime::parse_duration,
        help = "Background refresh interval (e.g. 30s, 5m)"
    )]
    pub refresh_interval: Duration,

    #[arg(
        long,
        env = "ECS_SD_METADATA_LEVEL",
        default_value = "task",
        value_parser = parse_metadata_level,
        help = "Metadata level: container, task, service, cluster, aws"
    )]
    pub metadata_level: MetadataLevel,

    #[arg(
        long,
        env = "ECS_SD_MODE",
        default_value = "discovery",
        help = "Operating mode: discovery (default) or proxy"
    )]
    pub mode: Mode,

    #[arg(
        long,
        env = "ECS_SD_PUBLIC_ADDRESS",
        help = "Reachable address of this ecs-sd instance (required in proxy mode)"
    )]
    pub public_address: Option<String>,

    #[arg(
        long,
        env = "ECS_SD_CLUSTER_MODE",
        default_value = "standalone",
        help = "Cluster mode: standalone (default) or cluster"
    )]
    pub cluster_mode: ClusterMode,

    #[arg(
        long,
        env = "ECS_SD_CLUSTER_SEEDS",
        help = "Comma-separated list of cluster seed addresses (host:port)"
    )]
    pub cluster_seeds: Option<String>,

    #[arg(
        long,
        env = "ECS_SD_GOSSIP_PORT",
        default_value = "8081",
        help = "UDP port for gossip protocol"
    )]
    pub gossip_port: u16,

    #[arg(
        long,
        env = "ECS_SD_NODE_ID",
        help = "Unique node ID in the cluster (defaults to HOSTNAME:gossip_port)"
    )]
    pub node_id: Option<String>,

    #[arg(
        long,
        env = "ECS_SD_METRICS_PORT",
        help = "Optional separate port for /metrics endpoint (defaults to --listen port)"
    )]
    pub metrics_port: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub clusters: Vec<String>,
    pub listen: String,
    pub refresh_interval: u64,
    pub metadata_level: MetadataLevel,
    pub mode: Mode,
    pub public_address: Option<String>,
    pub public_address_scheme: Option<String>,
    pub cluster_mode: ClusterMode,
    pub cluster_seeds: Vec<String>,
    pub gossip_port: u16,
    pub node_id: String,
    pub metrics_port: Option<u16>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            clusters: Vec::new(),
            listen: "0.0.0.0:8080".to_string(),
            refresh_interval: 60,
            metadata_level: MetadataLevel::default(),
            mode: Mode::Discovery,
            public_address: None,
            public_address_scheme: None,
            cluster_mode: ClusterMode::Standalone,
            cluster_seeds: Vec::new(),
            gossip_port: 8081,
            node_id: "localhost:8081".to_string(),
            metrics_port: None,
        }
    }
}

impl Config {
    pub fn from_process_args() -> Result<Self, ConfigError> {
        Self::from_iter(std::env::args_os())
    }

    pub fn from_iter<I, T>(iter: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let args = Args::try_parse_from(iter)
            .map_err(|err| ConfigError::InvalidValue(err.to_string()))?;

        Self::try_from_args(args)
    }

    fn try_from_args(args: Args) -> Result<Self, ConfigError> {
        let clusters: Vec<String> = args
            .clusters
            .split(',')
            .map(str::trim)
            .filter(|cluster| !cluster.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        if clusters.is_empty() {
            return Err(ConfigError::InvalidValue(
                "clusters must contain at least one non-empty entry".to_string(),
            ));
        }

        args.listen.parse::<SocketAddr>().map_err(|_| {
            ConfigError::InvalidValue(format!(
                "listen must be a valid socket address, got '{}'",
                args.listen
            ))
        })?;

        if args.refresh_interval <= Duration::ZERO {
            return Err(ConfigError::InvalidValue(
                "refresh interval must be greater than 0".to_string(),
            ));
        }

        let refresh_interval = args.refresh_interval.as_secs();
        if refresh_interval == 0 {
            return Err(ConfigError::InvalidValue(
                "refresh interval must be at least 1 second".to_string(),
            ));
        }

        let (public_address, public_address_scheme) =
            normalize_public_address(&args.mode, args.public_address.as_deref())?;

        let cluster_seeds: Vec<String> = if let Some(ref seeds_str) = args.cluster_seeds {
            seeds_str
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        } else {
            Vec::new()
        };

        for seed in &cluster_seeds {
            if let Some((_, port_str)) = seed.rsplit_once(':') {
                if port_str.parse::<u16>().is_err() {
                    return Err(ConfigError::InvalidValue(format!(
                        "invalid cluster seed '{}': must be host:port",
                        seed
                    )));
                }
            } else {
                return Err(ConfigError::InvalidValue(format!(
                    "invalid cluster seed '{}': must be host:port",
                    seed
                )));
            }
        }

        let node_id = args
            .node_id
            .unwrap_or_else(|| default_node_id(args.gossip_port));

        Ok(Self {
            clusters,
            listen: args.listen,
            refresh_interval,
            metadata_level: args.metadata_level,
            mode: args.mode,
            public_address,
            public_address_scheme,
            cluster_mode: args.cluster_mode,
            cluster_seeds,
            gossip_port: args.gossip_port,
            node_id,
            metrics_port: args.metrics_port,
        })
    }
}

fn default_node_id(gossip_port: u16) -> String {
    let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_string());
    format!("{}:{}", hostname, gossip_port)
}

fn parse_metadata_level(input: &str) -> Result<MetadataLevel, String> {
    MetadataLevel::from_str(input)
}

fn normalize_public_address(
    mode: &Mode,
    raw_public_address: Option<&str>,
) -> Result<(Option<String>, Option<String>), ConfigError> {
    let Some(raw_public_address) = raw_public_address else {
        if *mode == Mode::Proxy {
            return Err(ConfigError::InvalidValue(
                "--public-address / ECS_SD_PUBLIC_ADDRESS is required in proxy mode".to_string(),
            ));
        }
        return Ok((None, None));
    };

    let parsed = reqwest::Url::parse(raw_public_address).map_err(|_| {
        ConfigError::InvalidValue(format!(
            "invalid --public-address / ECS_SD_PUBLIC_ADDRESS '{}': expected full URL with http:// or https://",
            raw_public_address
        ))
    })?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(ConfigError::InvalidValue(format!(
            "invalid --public-address / ECS_SD_PUBLIC_ADDRESS '{}': only http:// or https:// are supported",
            raw_public_address
        )));
    }

    if parsed.username() != "" || parsed.password().is_some() {
        return Err(ConfigError::InvalidValue(format!(
            "invalid --public-address / ECS_SD_PUBLIC_ADDRESS '{}': user info is not allowed",
            raw_public_address
        )));
    }

    if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(ConfigError::InvalidValue(format!(
            "invalid --public-address / ECS_SD_PUBLIC_ADDRESS '{}': path, query, and fragment are not allowed",
            raw_public_address
        )));
    }

    let host = parsed.host_str().ok_or_else(|| {
        ConfigError::InvalidValue(format!(
            "invalid --public-address / ECS_SD_PUBLIC_ADDRESS '{}': host is required",
            raw_public_address
        ))
    })?;

    if host.parse::<IpAddr>().is_ok() {
        return Err(ConfigError::InvalidValue(format!(
            "invalid --public-address / ECS_SD_PUBLIC_ADDRESS '{}': host must be a domain name",
            raw_public_address
        )));
    }

    let port = parsed.port_or_known_default().ok_or_else(|| {
        ConfigError::InvalidValue(format!(
            "invalid --public-address / ECS_SD_PUBLIC_ADDRESS '{}': missing port and unknown default for scheme",
            raw_public_address
        ))
    })?;

    Ok((
        Some(format!("{}:{}", host, port)),
        Some(scheme.to_string()),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn clear_cluster_env_vars() {
        unsafe {
            std::env::remove_var("ECS_SD_CLUSTER_MODE");
            std::env::remove_var("ECS_SD_CLUSTER_SEEDS");
            std::env::remove_var("ECS_SD_GOSSIP_PORT");
            std::env::remove_var("ECS_SD_NODE_ID");
        }
    }

    fn clear_mode_env_vars() {
        unsafe {
            std::env::remove_var("ECS_SD_MODE");
            std::env::remove_var("ECS_SD_PUBLIC_ADDRESS");
        }
    }

    #[test]
    fn mode_defaults_to_discovery() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
        assert_eq!(config.mode, Mode::Discovery);
    }

    #[test]
    fn mode_flag_sets_proxy() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod", "--mode", "proxy", "--public-address", "http://ecs-sd.local:8080"])
            .expect("should succeed");
        assert_eq!(config.mode, Mode::Proxy);
        assert_eq!(config.public_address.as_deref(), Some("ecs-sd.local:8080"));
        assert_eq!(config.public_address_scheme.as_deref(), Some("http"));
    }

    #[test]
    fn env_ecs_sd_mode_sets_proxy() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::set_var("ECS_SD_MODE", "proxy");
            std::env::set_var("ECS_SD_PUBLIC_ADDRESS", "http://host.example:8080");
        }
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
        assert_eq!(config.mode, Mode::Proxy);
        assert_eq!(config.public_address.as_deref(), Some("host.example:8080"));
        assert_eq!(config.public_address_scheme.as_deref(), Some("http"));
        unsafe {
            std::env::remove_var("ECS_SD_MODE");
            std::env::remove_var("ECS_SD_PUBLIC_ADDRESS");
        }
    }

    #[test]
    fn proxy_mode_without_public_address_fails() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let err = Config::from_iter(["ecs-sd", "--clusters", "prod", "--mode", "proxy"])
            .expect_err("should fail");
        assert!(err.to_string().contains("--public-address"), "error was: {err}");
    }

    #[test]
    fn proxy_mode_with_public_address_succeeds() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter([
            "ecs-sd",
            "--clusters",
            "prod",
            "--mode",
            "proxy",
            "--public-address",
            "https://ecs-sd.example.com",
        ])
        .expect("should succeed");
        assert_eq!(config.public_address.as_deref(), Some("ecs-sd.example.com:443"));
        assert_eq!(config.public_address_scheme.as_deref(), Some("https"));
    }

    #[test]
    fn discovery_mode_without_public_address_ok() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
    }

    #[test]
    fn public_address_requires_scheme() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let err = Config::from_iter([
            "ecs-sd",
            "--clusters",
            "prod",
            "--mode",
            "proxy",
            "--public-address",
            "ecs-sd.example.com:8080",
        ])
        .expect_err("should reject missing scheme");
        assert!(err.to_string().contains("http:// or https://"), "error was: {err}");
    }

    #[test]
    fn public_address_rejects_unsupported_scheme() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let err = Config::from_iter([
            "ecs-sd",
            "--clusters",
            "prod",
            "--mode",
            "proxy",
            "--public-address",
            "ftp://ecs-sd.example.com:8080",
        ])
        .expect_err("should reject unsupported scheme");
        assert!(err.to_string().contains("only http:// or https://"), "error was: {err}");
    }

    #[test]
    fn public_address_rejects_ip_host() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let err = Config::from_iter([
            "ecs-sd",
            "--clusters",
            "prod",
            "--mode",
            "proxy",
            "--public-address",
            "http://10.0.0.10:8080",
        ])
        .expect_err("should reject IP host");
        assert!(err.to_string().contains("host must be a domain name"), "error was: {err}");
    }

    #[test]
    fn public_address_rejects_path_query_and_fragment() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let err = Config::from_iter([
            "ecs-sd",
            "--clusters",
            "prod",
            "--mode",
            "proxy",
            "--public-address",
            "https://ecs-sd.example.com/proxy?x=1#frag",
        ])
        .expect_err("should reject path/query/fragment");
        assert!(
            err.to_string().contains("path, query, and fragment are not allowed"),
            "error was: {err}"
        );
    }

    #[test]
    fn invalid_mode_rejected() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        Config::from_iter(["ecs-sd", "--clusters", "prod", "--mode", "invalid"])
            .expect_err("should reject unknown mode");
    }

    #[test]
    fn cli_overrides_env_refresh_interval() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::set_var("ECS_SD_CLUSTERS", "from-env");
            std::env::set_var("ECS_SD_REFRESH_INTERVAL", "120s");
            std::env::remove_var("ECS_SD_LISTEN");
            std::env::remove_var("ECS_SD_METADATA_LEVEL");
        }
        clear_mode_env_vars();
        clear_cluster_env_vars();

        let result = Config::from_iter(["ecs-sd", "--clusters", "from-cli", "--refresh-interval", "30s"])
            .expect("config parsing should succeed");

        assert_eq!(result.clusters, vec!["from-cli"]);
        assert_eq!(result.refresh_interval, 30);

        unsafe {
            std::env::remove_var("ECS_SD_CLUSTERS");
            std::env::remove_var("ECS_SD_REFRESH_INTERVAL");
        }
    }

    #[test]
    fn uses_env_when_cli_absent() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::set_var("ECS_SD_CLUSTERS", "prod,staging");
            std::env::set_var("ECS_SD_LISTEN", "127.0.0.1:18080");
            std::env::set_var("ECS_SD_REFRESH_INTERVAL", "45s");
            std::env::set_var("ECS_SD_METADATA_LEVEL", "service");
        }
        clear_mode_env_vars();
        clear_cluster_env_vars();

        let result = Config::from_iter(["ecs-sd"]).expect("config parsing should succeed");

        assert_eq!(result.clusters, vec!["prod", "staging"]);
        assert_eq!(result.listen, "127.0.0.1:18080");
        assert_eq!(result.refresh_interval, 45);
        assert_eq!(result.metadata_level, MetadataLevel::Service);

        unsafe {
            std::env::remove_var("ECS_SD_CLUSTERS");
            std::env::remove_var("ECS_SD_LISTEN");
            std::env::remove_var("ECS_SD_REFRESH_INTERVAL");
            std::env::remove_var("ECS_SD_METADATA_LEVEL");
        }
    }

    #[test]
    fn uses_defaults_when_optional_values_absent() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::set_var("ECS_SD_CLUSTERS", "prod");
            std::env::remove_var("ECS_SD_LISTEN");
            std::env::remove_var("ECS_SD_REFRESH_INTERVAL");
            std::env::remove_var("ECS_SD_METADATA_LEVEL");
        }
        clear_mode_env_vars();
        clear_cluster_env_vars();

        let result = Config::from_iter(["ecs-sd"]).expect("config parsing should succeed");

        assert_eq!(result.clusters, vec!["prod"]);
        assert_eq!(result.listen, "0.0.0.0:8080");
        assert_eq!(result.refresh_interval, 60);
        assert_eq!(result.metadata_level, MetadataLevel::Task);

        unsafe {
            std::env::remove_var("ECS_SD_CLUSTERS");
        }
    }

    #[test]
    fn rejects_empty_clusters() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_cluster_env_vars();
        let error = Config::from_iter(["ecs-sd", "--clusters", " , , "]).expect_err("should reject empty clusters");
        assert!(error.to_string().contains("clusters must contain at least one non-empty entry"));
    }

    #[test]
    fn rejects_invalid_listen_address() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_cluster_env_vars();
        let error = Config::from_iter(["ecs-sd", "--clusters", "prod", "--listen", "bad-listen"])
            .expect_err("should reject invalid listen");
        assert!(error.to_string().contains("listen must be a valid socket address"));
    }

    #[test]
    fn rejects_zero_refresh_interval() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_cluster_env_vars();
        let error = Config::from_iter(["ecs-sd", "--clusters", "prod", "--refresh-interval", "0s"])
            .expect_err("should reject zero refresh interval");
        assert!(error.to_string().contains("refresh interval must be greater than 0"));
    }

    // ---- Cluster config tests ----

    #[test]
    fn cluster_mode_defaults_to_standalone() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
        assert_eq!(config.cluster_mode, ClusterMode::Standalone);
    }

    #[test]
    fn cluster_mode_cluster_via_flag() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod", "--cluster-mode", "cluster"])
            .expect("should succeed");
        assert_eq!(config.cluster_mode, ClusterMode::Cluster);
    }

    #[test]
    fn cluster_mode_cluster_via_env() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        unsafe {
            std::env::set_var("ECS_SD_CLUSTER_MODE", "cluster");
        }
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
        assert_eq!(config.cluster_mode, ClusterMode::Cluster);
        unsafe {
            std::env::remove_var("ECS_SD_CLUSTER_MODE");
        }
    }

    #[test]
    fn gossip_port_defaults_to_8081() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
        assert_eq!(config.gossip_port, 8081);
    }

    #[test]
    fn gossip_port_overridable() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod", "--gossip-port", "9999"])
            .expect("should succeed");
        assert_eq!(config.gossip_port, 9999);
    }

    #[test]
    fn cluster_seeds_parsed_from_comma_separated() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter([
            "ecs-sd",
            "--clusters", "prod",
            "--cluster-mode", "cluster",
            "--cluster-seeds", "host1:8081,host2:8081",
        ])
        .expect("should succeed");
        assert_eq!(config.cluster_seeds, vec!["host1:8081", "host2:8081"]);
    }

    #[test]
    fn cluster_seeds_empty_allowed_in_cluster_mode() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod", "--cluster-mode", "cluster"])
            .expect("should succeed");
        assert_eq!(config.cluster_seeds, Vec::<String>::new());
    }

    #[test]
    fn node_id_defaults_to_hostname_colon_port() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
        assert!(
            config.node_id.contains(':'),
            "node_id should contain colon: {}",
            config.node_id
        );
        assert!(
            config.node_id.ends_with(":8081"),
            "node_id should end with default gossip port: {}",
            config.node_id
        );
    }

    #[test]
    fn node_id_overridable() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod", "--node-id", "my-custom-id"])
            .expect("should succeed");
        assert_eq!(config.node_id, "my-custom-id");
    }

    #[test]
    fn invalid_cluster_seed_rejected() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let err = Config::from_iter(["ecs-sd", "--clusters", "prod", "--cluster-seeds", "notaport"])
            .expect_err("should fail");
        assert!(err.to_string().contains("invalid cluster seed"), "error was: {err}");
    }

    #[test]
    fn metrics_port_defaults_to_none() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
        assert_eq!(config.metrics_port, None);
    }

    #[test]
    fn metrics_port_overridable_via_flag() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod", "--metrics-port", "9090"])
            .expect("should succeed");
        assert_eq!(config.metrics_port, Some(9090));
    }

    #[test]
    fn metrics_port_overridable_via_env() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_mode_env_vars();
        clear_cluster_env_vars();
        unsafe {
            std::env::set_var("ECS_SD_METRICS_PORT", "9091");
        }
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
        assert_eq!(config.metrics_port, Some(9091));
        unsafe {
            std::env::remove_var("ECS_SD_METRICS_PORT");
        }
    }
}
