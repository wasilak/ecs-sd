use std::ffi::OsString;
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use clap::Parser;

use crate::error::ConfigError;
use crate::models::MetadataLevel;

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
}

#[derive(Debug, Clone)]
pub struct Config {
    pub clusters: Vec<String>,
    pub listen: String,
    pub refresh_interval: u64,
    pub metadata_level: MetadataLevel,
    pub mode: Mode,
    pub public_address: Option<String>,
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

        if args.mode == Mode::Proxy && args.public_address.is_none() {
            return Err(ConfigError::InvalidValue(
                "--public-address / ECS_SD_PUBLIC_ADDRESS is required in proxy mode".to_string(),
            ));
        }

        Ok(Self {
            clusters,
            listen: args.listen,
            refresh_interval,
            metadata_level: args.metadata_level,
            mode: args.mode,
            public_address: args.public_address,
        })
    }
}

fn parse_metadata_level(input: &str) -> Result<MetadataLevel, String> {
    MetadataLevel::from_str(input)
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn mode_defaults_to_discovery() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::remove_var("ECS_SD_MODE");
            std::env::remove_var("ECS_SD_PUBLIC_ADDRESS");
        }
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
        assert_eq!(config.mode, Mode::Discovery);
    }

    #[test]
    fn mode_flag_sets_proxy() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::remove_var("ECS_SD_MODE");
            std::env::remove_var("ECS_SD_PUBLIC_ADDRESS");
        }
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod", "--mode", "proxy", "--public-address", "ecs-sd.local:8080"])
            .expect("should succeed");
        assert_eq!(config.mode, Mode::Proxy);
    }

    #[test]
    fn env_ecs_sd_mode_sets_proxy() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::set_var("ECS_SD_MODE", "proxy");
            std::env::set_var("ECS_SD_PUBLIC_ADDRESS", "host:8080");
        }
        let config = Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
        assert_eq!(config.mode, Mode::Proxy);
        unsafe {
            std::env::remove_var("ECS_SD_MODE");
            std::env::remove_var("ECS_SD_PUBLIC_ADDRESS");
        }
    }

    #[test]
    fn proxy_mode_without_public_address_fails() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::remove_var("ECS_SD_MODE");
            std::env::remove_var("ECS_SD_PUBLIC_ADDRESS");
        }
        let err = Config::from_iter(["ecs-sd", "--clusters", "prod", "--mode", "proxy"])
            .expect_err("should fail");
        assert!(err.to_string().contains("--public-address"), "error was: {err}");
    }

    #[test]
    fn proxy_mode_with_public_address_succeeds() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::remove_var("ECS_SD_MODE");
            std::env::remove_var("ECS_SD_PUBLIC_ADDRESS");
        }
        Config::from_iter(["ecs-sd", "--clusters", "prod", "--mode", "proxy", "--public-address", "10.0.0.1:8080"])
            .expect("should succeed");
    }

    #[test]
    fn discovery_mode_without_public_address_ok() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::remove_var("ECS_SD_MODE");
            std::env::remove_var("ECS_SD_PUBLIC_ADDRESS");
        }
        Config::from_iter(["ecs-sd", "--clusters", "prod"]).expect("should succeed");
    }

    #[test]
    fn invalid_mode_rejected() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::remove_var("ECS_SD_MODE");
            std::env::remove_var("ECS_SD_PUBLIC_ADDRESS");
        }
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
        let error = Config::from_iter(["ecs-sd", "--clusters", " , , "]).expect_err("should reject empty clusters");
        assert!(error.to_string().contains("clusters must contain at least one non-empty entry"));
    }

    #[test]
    fn rejects_invalid_listen_address() {
        let error = Config::from_iter(["ecs-sd", "--clusters", "prod", "--listen", "bad-listen"])
            .expect_err("should reject invalid listen");
        assert!(error.to_string().contains("listen must be a valid socket address"));
    }

    #[test]
    fn rejects_zero_refresh_interval() {
        let error = Config::from_iter(["ecs-sd", "--clusters", "prod", "--refresh-interval", "0s"])
            .expect_err("should reject zero refresh interval");
        assert!(error.to_string().contains("refresh interval must be greater than 0"));
    }
}
