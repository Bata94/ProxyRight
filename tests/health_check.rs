use echoserver::config::{Config, Route, HealthStatus};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_health_check_healthy() {
    let config = Config {
        routes: vec![Arc::new(Mutex::new(Route {
            host: "test.localhost".to_string(),
            backends: vec!["http://127.0.0.1:12345".to_string()],
            health_check_urls: Some(vec!["http://127.0.0.1:12345/health".to_string()]),
            statuses: vec![HealthStatus::Unhealthy],
            current_backend_index: Arc::new(AtomicUsize::new(0)),
        }))],
        health_check_interval: 1,
        default_route: None,
        static_files: vec![],
        middlewares: vec![],
        instantiated_middlewares: vec![],
    };

    let route = config.routes[0].clone();
    let client = reqwest::Client::new();

    echoserver::health::check_health(&client, route.clone(), 0).await;

    let route_lock = route.lock().await;
    assert_eq!(route_lock.statuses[0], HealthStatus::Unhealthy); // Expect unhealthy as no server is running
}

#[tokio::test]
async fn test_health_check_unhealthy() {
    let config = Config {
        routes: vec![Arc::new(Mutex::new(Route {
            host: "test.localhost".to_string(),
            backends: vec!["http://127.0.0.1:12345".to_string()],
            health_check_urls: Some(vec!["http://127.0.0.1:12345/health".to_string()]),
            statuses: vec![HealthStatus::Healthy],
            current_backend_index: Arc::new(AtomicUsize::new(0)),
        }))],
        health_check_interval: 1,
        default_route: None,
        static_files: vec![],
        middlewares: vec![],
        instantiated_middlewares: vec![],
    };

    let route = config.routes[0].clone();
    let client = reqwest::Client::new();

    echoserver::health::check_health(&client, route.clone(), 0).await;

    let route_lock = route.lock().await;
    assert_eq!(route_lock.statuses[0], HealthStatus::Unhealthy); // Expect unhealthy as no server is running
}
