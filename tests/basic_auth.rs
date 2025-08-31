use hyper::{Body, Request, Response, StatusCode};
use hyper::client::Client;
use hyper::Uri;
use tokio::runtime::Runtime;
use std::process::{Child, Command};
use std::time::Duration;
use tokio::time::sleep;
use std::io::{BufReader, BufRead};
use std::fs;

const CONFIG_WITH_BASIC_AUTH: &str = r#"
health_check_interval = 5

[default_route]
backend = "http://127.0.0.1:5500"

[basic_auth]
username = "testuser"
password = "testpass"
"#;

const CONFIG_WITHOUT_BASIC_AUTH: &str = r#"
health_check_interval = 5

[default_route]
backend = "http://127.0.0.1:5500"
"#;

async fn start_server_with_config(config_content: &str) -> (Child, u16) {
    let config_path = "test_config.toml";
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
async fn test_basic_auth_middleware() {
    // Start a dummy backend server
    let (mut backend_child, backend_port) = start_dummy_backend().await;
    let backend_url = format!("http://127.0.0.1:{}", backend_port);
    println!("Dummy backend started on {}", backend_url);

    // Test with basic auth enabled
    let (mut proxy_child_auth, proxy_port_auth) = start_server_with_config(CONFIG_WITH_BASIC_AUTH).await;
    let proxy_url_auth = format!("http://127.0.0.1:{}", proxy_port_auth);
    println!("Proxy with auth started on {}", proxy_url_auth);

    let client = Client::new();

    // Test 1: No Authorization header - Expect 401
    println!("Test 1: No Authorization header");
    let req = Request::builder()
        .uri(format!("{}/", proxy_url_auth))
        .body(Body::empty())
        .unwrap();
    let res = client.request(req).await.unwrap();
    println!("Response status: {}", res.status());
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Test 2: Incorrect credentials - Expect 401
    println!("Test 2: Incorrect credentials");
    let req = Request::builder()
        .uri(format!("{}/", proxy_url_auth))
        .header(hyper::header::AUTHORIZATION, "Basic YWRtaW46d3JvbmdwYXNz") // admin:wrongpass
        .body(Body::empty())
        .unwrap();
    let res = client.request(req).await.unwrap();
    println!("Response status: {}", res.status());
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Test 3: Correct credentials - Expect 200
    println!("Test 3: Correct credentials");
    let req = Request::builder()
        .uri(format!("{}/", proxy_url_auth))
        .header(hyper::header::AUTHORIZATION, "Basic dGVzdHVzZXI6dGVzdHBhc3M=") // testuser:testpass
        .body(Body::empty())
        .unwrap();
    let res = client.request(req).await.unwrap();
    println!("Response status: {}", res.status());
    assert_eq!(res.status(), StatusCode::OK);

    // Test with basic auth disabled
    let (mut proxy_child_no_auth, proxy_port_no_auth) = start_server_with_config(CONFIG_WITHOUT_BASIC_AUTH).await;
    let proxy_url_no_auth = format!("http://127.0.0.1:{}", proxy_port_no_auth);
    println!("Proxy without auth started on {}", proxy_url_no_auth);

    // Test 4: No Authorization header (auth disabled) - Expect 200
    println!("Test 4: No Authorization header (auth disabled)");
    let req = Request::builder()
        .uri(format!("{}/", proxy_url_no_auth))
        .body(Body::empty())
        .unwrap();
    let res = client.request(req).await.unwrap();
    println!("Response status: {}", res.status());
    assert_eq!(res.status(), StatusCode::OK);

    // Cleanup
    proxy_child_auth.kill().expect("Failed to kill proxy_child_auth");
    proxy_child_no_auth.kill().expect("Failed to kill proxy_child_no_auth");
    backend_child.kill().expect("Failed to kill backend_child");
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

