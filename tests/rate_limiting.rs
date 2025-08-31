use hyper::{Body, Request, Response, StatusCode};
use hyper::client::Client;
use hyper::Uri;
use tokio::runtime::Runtime;
use std::process::{Child, Command};
use std::time::Duration;
use tokio::time::sleep;
use std::io::{BufReader, BufRead};
use std::fs;

const CONFIG_WITH_RATE_LIMIT: &str = r#"
health_check_interval = 5

[default_route]
backend = "http://127.0.0.1:5500"

[rate_limit]
rps = 1
burst = 1
"#;

const CONFIG_WITHOUT_RATE_LIMIT: &str = r#"
health_check_interval = 5

[default_route]
backend = "http://127.0.0.1:5500"
"#;

async fn start_server_with_config(config_content: &str) -> (Child, u16) {
    let config_path = "test_config_rate_limit.toml";
    fs::write(config_path, config_content).expect("Unable to write config file");

    let port = find_available_port().await;
    let addr = format!("127.0.0.1:{}", port);

    println!("Starting proxy server on {} with config: \n{}", addr, config_content);
    let child = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg(&addr)
        .env("CONFIG_PATH", config_path)
        .spawn()
        .expect("Failed to start server");

    // Give the server some time to start up and become ready
    wait_for_server_ready(&addr).await;
    println!("Proxy server started on port {}", port);

    (child, port)
}

async fn wait_for_server_ready(addr: &str) {
    let client = Client::new();
    let full_uri = format!("http://{}", addr);
    let uri = full_uri.parse::<Uri>().unwrap();
    for i in 0..10 { // Try for up to 10 seconds
        match client.get(uri.clone()).await {
            Ok(_) => {
                println!("Server at {} is ready after {} attempts.", addr, i + 1);
                return;
            },
            Err(e) => {
                println!("Waiting for server at {}: Attempt {} failed with error: {}", addr, i + 1, e);
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
    panic!("Server at {} did not become ready within timeout", addr);
}

async fn find_available_port() -> u16 {
    use std::net::TcpListener;
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

#[tokio::test]
async fn test_rate_limiting_middleware() {
    // Start a dummy backend server
    let (mut backend_child, backend_port) = start_dummy_backend().await;
    let backend_url = format!("http://127.0.0.1:{}", backend_port);
    println!("Dummy backend started on {}", backend_url);

    // Test with rate limit enabled (1 rps, 1 burst)
    let (mut proxy_child_rate_limit, proxy_port_rate_limit) = start_server_with_config(CONFIG_WITH_RATE_LIMIT).await;
    let proxy_url_rate_limit = format!("http://127.0.0.1:{}", proxy_port_rate_limit);
    println!("Proxy with rate limit started on {}", proxy_url_rate_limit);

    let client = Client::new();

    // Test 1: First request (actually second due to readiness check) - Expect 429
    println!("Test 1: First request (actually second due to readiness check)");
    let req = Request::builder()
        .uri(format!("{}/", proxy_url_rate_limit))
        .body(Body::empty())
        .unwrap();
    let res = client.request(req).await.unwrap();
    println!("Response status: {}", res.status());
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);

    // Test 2: Second request immediately (actually third) - Expect 429
    println!("Test 2: Second request immediately (actually third)");
    let req = Request::builder()
        .uri(format!("{}/", proxy_url_rate_limit))
        .body(Body::empty())
        .unwrap();
    let res = client.request(req).await.unwrap();
    println!("Response status: {}", res.status());
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);

    // Test 3: Wait for 1 second, then request - Expect 200
    println!("Test 3: Wait for 1 second, then request");
    sleep(Duration::from_secs(1)).await;
    let req = Request::builder()
        .uri(format!("{}/", proxy_url_rate_limit))
        .body(Body::empty())
        .unwrap();
    let res = client.request(req).await.unwrap();
    println!("Response status: {}", res.status());
    assert_eq!(res.status(), StatusCode::OK);

    // Test with rate limit disabled
    let (mut proxy_child_no_rate_limit, proxy_port_no_rate_limit) = start_server_with_config(CONFIG_WITHOUT_RATE_LIMIT).await;
    let proxy_url_no_rate_limit = format!("http://127.0.0.1:{}", proxy_port_no_rate_limit);
    println!("Proxy without rate limit started on {}", proxy_url_no_rate_limit);

    // Test 4: Multiple requests immediately (rate limit disabled) - Expect 200
    println!("Test 4: Multiple requests immediately (rate limit disabled)");
    let req1 = Request::builder()
        .uri(format!("{}/", proxy_url_no_rate_limit))
        .body(Body::empty())
        .unwrap();
    let res1 = client.request(req1).await.unwrap();
    println!("Response status 1: {}", res1.status());
    assert_eq!(res1.status(), StatusCode::OK);

    let req2 = Request::builder()
        .uri(format!("{}/", proxy_url_no_rate_limit))
        .body(Body::empty())
        .unwrap();
    let res2 = client.request(req2).await.unwrap();
    println!("Response status 2: {}", res2.status());
    assert_eq!(res2.status(), StatusCode::OK);

    // Cleanup
    proxy_child_rate_limit.kill().expect("Failed to kill proxy_child_rate_limit");
    proxy_child_no_rate_limit.kill().expect("Failed to kill proxy_child_no_rate_limit");
    backend_child.kill().expect("Failed to kill backend_child");
}

#[tokio::test]
async fn test_governor_direct() {
    use governor::{Quota, RateLimiter};
    use governor::state::keyed::DashMapStateStore;
    use governor::clock::Clock;
    use std::num::NonZeroU32;
    use std::net::IpAddr;
    use std::time::Duration;
    use tokio::time::sleep;
    use std::sync::Arc;

    let rps = 1;
    let burst = 1;
    let quota = Quota::per_second(NonZeroU32::new(rps).unwrap()).allow_burst(NonZeroU32::new(burst).unwrap());
    let limiter = Arc::new(RateLimiter::keyed(quota));

    let ip: IpAddr = "127.0.0.1".parse().unwrap();

    // First request - should be allowed
    println!("Direct Test: First request");
    assert!(limiter.check_key(&ip).is_ok());

    // Second request immediately - should be denied
    println!("Direct Test: Second request immediately");
    assert!(limiter.check_key(&ip).is_err());

    // Wait for 1 second - should replenish tokens
    println!("Direct Test: Waiting for 1 second");
    sleep(Duration::from_secs(1)).await;

    // Third request - should be allowed
    println!("Direct Test: Third request after 1 second");
    assert!(limiter.check_key(&ip).is_ok());
}

async fn start_dummy_backend() -> (Child, u16) {
    let port = find_available_port().await;
    let addr = format!("127.0.0.1:{}", port);

    println!("Starting dummy backend on {}", addr);
    let child = Command::new("python3")
        .arg("-m")
        .arg("http.server")
        .arg(port.to_string())
        .spawn()
        .expect("Failed to start dummy backend");

    sleep(Duration::from_secs(5)).await;
    println!("Dummy backend started on port {}", port);
    (child, port)
}
