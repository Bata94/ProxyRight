use serde::Deserialize;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tokio::sync::Mutex;

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
}

#[derive(Clone)]
pub struct Config {
    pub routes: Vec<Arc<Mutex<Route>>>,
    pub health_check_interval: u64,
    pub default_route: Option<Arc<Mutex<Route>>>,
    pub static_files: Vec<StaticRoute>,
    pub middlewares: Vec<String>,
    pub instantiated_middlewares: Vec<Arc<dyn crate::middleware::Middleware>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct StaticRoute {
    pub path: String,
    pub root: String,
    pub host: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Route {
    pub host: String,
    pub backends: Vec<String>,
    pub health_check_urls: Option<Vec<String>>,
    #[serde(default = "default_statuses")]
    pub statuses: Vec<HealthStatus>,
    #[serde(skip)]
    pub current_backend_index: Arc<AtomicUsize>,
}

fn default_statuses() -> Vec<HealthStatus> {
    vec![HealthStatus::Healthy]
}

#[derive(Deserialize, Debug, Clone)]
pub struct BasicAuthConfig {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RateLimitConfig {
    pub rps: u32,
    pub burst: u32,
}

pub fn load_config(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(path)?;
    let raw_config: RawConfig = toml::from_str(&contents)?;

    let routes = raw_config.routes.unwrap_or_default().into_iter().map(|raw_r| {
        let backends = if let Some(b) = raw_r.backends {
            b
        } else if let Some(b) = raw_r.backend {
            vec![b]
        } else {
            vec![]
        };

        let health_check_urls = if let Some(urls) = raw_r.health_check_urls {
            urls
        } else if let Some(url) = raw_r.health_check_url {
            vec![url]
        } else {
            backends.iter().map(|b| format!("{}/health", b)).collect()
        };

        let statuses = backends.iter().map(|_| HealthStatus::Healthy).collect();

        Arc::new(Mutex::new(Route {
            host: raw_r.host,
            backends,
            health_check_urls: Some(health_check_urls),
            statuses,
            current_backend_index: Arc::new(AtomicUsize::new(0)),
        }))
    }).collect();

    let default_route = if let Some(raw_r) = raw_config.default_route {
        let backends = if let Some(b) = raw_r.backends {
            b
        } else if let Some(b) = raw_r.backend {
            vec![b]
        } else {
            vec![]
        };

        let health_check_urls = if let Some(urls) = raw_r.health_check_urls {
            urls
        } else if let Some(url) = raw_r.health_check_url {
            vec![url]
        } else {
            backends.iter().map(|b| format!("{}/health", b)).collect()
        };

        let statuses = backends.iter().map(|_| HealthStatus::Healthy).collect();

        Some(Arc::new(Mutex::new(Route {
            host: "".to_string(), // Set a dummy host for the default route
            backends,
            health_check_urls: Some(health_check_urls),
            statuses,
            current_backend_index: Arc::new(AtomicUsize::new(0)),
        })))
    } else {
        None
    };

    let mut instantiated_middlewares: Vec<Arc<dyn crate::middleware::Middleware>> = Vec::new();

    if let Some(basic_auth_config) = raw_config.basic_auth {
        instantiated_middlewares.push(Arc::new(crate::middlewares::BasicAuthMiddleware::new(
            basic_auth_config.username,
            basic_auth_config.password,
        )));
    }

    if let Some(rate_limit_config) = raw_config.rate_limit {
        instantiated_middlewares.push(Arc::new(crate::middlewares::RateLimitingMiddleware::new(
            rate_limit_config.rps,
            rate_limit_config.burst,
        )));
    }

    for middleware_name in raw_config.middlewares.clone().unwrap_or_default() {
        match middleware_name.as_str() {
            "logging" => instantiated_middlewares.push(Arc::new(crate::middlewares::LoggingMiddleware)),
            _ => eprintln!("Unknown middleware: {}", middleware_name),
        }
    }

    Ok(Config {
        routes,
        health_check_interval: raw_config.health_check_interval.unwrap_or(10),
        default_route,
        static_files: raw_config.static_files.unwrap_or_default(),
        middlewares: raw_config.middlewares.unwrap_or_default(), // Keep this for backward compatibility if needed, or remove if all middlewares are instantiated
        instantiated_middlewares,
    })
}

#[derive(Deserialize)]
struct RawConfig {
    routes: Option<Vec<RawRoute>>,
    health_check_interval: Option<u64>,
    default_route: Option<DefaultRawRoute>,
    static_files: Option<Vec<StaticRoute>>,
    middlewares: Option<Vec<String>>,
    basic_auth: Option<BasicAuthConfig>,
    rate_limit: Option<RateLimitConfig>,
}

#[derive(Deserialize)]
struct DefaultRawRoute {
    backend: Option<String>,
    backends: Option<Vec<String>>,
    health_check_url: Option<String>,
    health_check_urls: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct RawRoute {
    host: String,
    backend: Option<String>,
    backends: Option<Vec<String>>,
    health_check_url: Option<String>,
    health_check_urls: Option<Vec<String>>,
}
