use ProxyRight::config::{Config, Route, HealthStatus};
use hyper::{Body, Response, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tokio::sync::Mutex;

async fn spawn_app(config: Config) -> SocketAddr {
    let config = Arc::new(config);
    let addr: SocketAddr = "127.0.0.1:0".parse().expect("Invalid address");

    let make_svc = make_service_fn(move |_conn| {
        let config = config.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                ProxyRight::handle_request(req, config.clone())
            }))
        }
    });

    let server = hyper::Server::bind(&addr).serve(make_svc);
    let socket_addr = server.local_addr();

    tokio::spawn(async move {
        server.await.unwrap();
    });

    socket_addr
}

#[tokio::test]
async fn test_routing_to_non_existent_host() {
        let config = Config {
        routes: vec![],
        health_check_interval: 1,
        default_route: None,
        static_files: vec![],
        middlewares: vec![],
        instantiated_middlewares: vec![],
    };

    let addr = spawn_app(config).await;
    let client = reqwest::Client::new();

    let response = client.get(&format!("http://{}/", addr))
        .header("Host", "nonexistent.localhost")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_routing_to_unhealthy_backend() {
    let config = Config {
        routes: vec![Arc::new(Mutex::new(Route {
            host: "unhealthy.localhost".to_string(),
            backends: vec!["http://127.0.0.1:9999".to_string()], // Non-existent backend
            health_check_urls: None,
            statuses: vec![HealthStatus::Unhealthy],
            current_backend_index: Arc::new(AtomicUsize::new(0)),
        }))],
        health_check_interval: 1,
        default_route: None,
        static_files: vec![],
        middlewares: vec![],
        instantiated_middlewares: vec![],
    };

    let addr = spawn_app(config).await;
    let client = reqwest::Client::new();

    let response = client.get(&format!("http://{}/", addr))
        .header("Host", "unhealthy.localhost")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_round_robin_routing() {
    let config = Config {
        routes: vec![Arc::new(Mutex::new(Route {
            host: "roundrobin.localhost".to_string(),
            backends: vec!["http://127.0.0.1:8001".to_string(), "http://127.0.0.1:8002".to_string()],
            health_check_urls: None,
            statuses: vec![HealthStatus::Healthy, HealthStatus::Healthy],
            current_backend_index: Arc::new(AtomicUsize::new(0)),
        }))],
        health_check_interval: 1,
        default_route: None,
        static_files: vec![],
        middlewares: vec![],
        instantiated_middlewares: vec![],
    };

    let addr = spawn_app(config).await;
    let client = reqwest::Client::new();

    // Mock backends
    let backend1_addr: SocketAddr = "127.0.0.1:8001".parse().unwrap();
    let backend2_addr: SocketAddr = "127.0.0.1:8002".parse().unwrap();

    let backend1_server = hyper::Server::bind(&backend1_addr).serve(make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(|_req| async {
            Ok::<_, Infallible>(Response::new(Body::from("Backend 1")))
        }))
    }));
    tokio::spawn(async move { backend1_server.await.unwrap(); });

    let backend2_server = hyper::Server::bind(&backend2_addr).serve(make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(|_req| async {
            Ok::<_, Infallible>(Response::new(Body::from("Backend 2")))
        }))
    }));
    tokio::spawn(async move { backend2_server.await.unwrap(); });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await; // Give servers time to start

    let response1 = client.get(&format!("http://{}/", addr))
        .header("Host", "roundrobin.localhost")
        .send()
        .await
        .expect("Failed to send request 1");
    assert_eq!(response1.status(), StatusCode::OK);
    assert_eq!(response1.text().await.unwrap(), "Backend 1");

    let response2 = client.get(&format!("http://{}/", addr))
        .header("Host", "roundrobin.localhost")
        .send()
        .await
        .expect("Failed to send request 2");
    assert_eq!(response2.status(), StatusCode::OK);
    assert_eq!(response2.text().await.unwrap(), "Backend 2");

    let response3 = client.get(&format!("http://{}/", addr))
        .header("Host", "roundrobin.localhost")
        .send()
        .await
        .expect("Failed to send request 3");
    assert_eq!(response3.status(), StatusCode::OK);
    assert_eq!(response3.text().await.unwrap(), "Backend 1");
}
