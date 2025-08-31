use hyper::{Body, Response, Server, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::task::JoinHandle;
use reqwest::Client;
use std::fs;
use std::io::Write;
use std::sync::Arc;

// Helper to spawn the main application with a specific config file
async fn spawn_app_with_config(config: echoserver::config::Config) -> (SocketAddr, JoinHandle<()>) {
    let addr: SocketAddr = "127.0.0.1:0".parse().expect("Invalid address"); // Use 0 for ephemeral port

    let (tx, rx) = tokio::sync::oneshot::channel();

    let config_arc = Arc::new(config);

    let handle = tokio::spawn(async move {
        let make_svc = make_service_fn(move |_conn| {
            let config_clone = config_arc.clone(); // Clone Arc for each service
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    echoserver::handle_request(req, config_clone.clone()) // Clone Arc for each request
                }))
            }
        });
        let server = Server::bind(&addr).serve(make_svc);
        tx.send(server.local_addr()).unwrap();
        server.await.unwrap();
    });

    let socket_addr = rx.await.unwrap();

    // Give the server a moment to start up
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    (socket_addr, handle)
}

// Mock backend server
async fn mock_backend() -> (SocketAddr, JoinHandle<()>) {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();

    let handle = tokio::spawn(async move {
        let make_svc = make_service_fn(|_conn| async {
            Ok::<_, Infallible>(service_fn(move |req| {
                async move {
                    let response_body = format!("Mock Backend Response for {}", req.uri().path());
                    Ok::<_, Infallible>(Response::new(Body::from(response_body)))
                }
            }))
        });
        let server = Server::bind(&addr).serve(make_svc);
        tx.send(server.local_addr()).unwrap();
        server.await.unwrap();
    });

    let socket_addr = rx.await.unwrap();
    (socket_addr, handle)
}

#[tokio::test]
async fn test_basic_proxying() {
    // 1. Start a mock backend server
    let (backend_addr, backend_handle) = mock_backend().await;
    let backend_port = backend_addr.port();

    // 2. Create a temporary config file
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("test_config.toml");
    let config_content = format!(
        r#" 
health_check_interval = 1

[[routes]]
host = "test.localhost"
backends = ["http://127.0.0.1:{}"]
"#,
        backend_port
    );
    let mut file = fs::File::create(&config_path).unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    let config = echoserver::config::load_config(config_path.to_str().unwrap()).unwrap();

    // 3. Start the proxy application with the temporary config
    let (proxy_addr, proxy_handle) = spawn_app_with_config(config).await;

    // 4. Send a request to the proxy
    let client = Client::new();
    let proxy_url = format!("http://{}/{}", proxy_addr, "test_path");
    let response = client.get(&proxy_url)
        .header("Host", "test.localhost")
        .send()
        .await
        .expect("Failed to send request to proxy");

    // 5. Verify the response
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("Failed to get response body");
    assert_eq!(body, "Mock Backend Response for /test_path");

    // Clean up
    backend_handle.abort();
    proxy_handle.abort();
    temp_dir.close().unwrap();
}

#[tokio::test]
async fn test_default_proxying() {
    // 1. Start a mock backend server for the default route
    let (backend_addr, backend_handle) = mock_backend().await;
    let backend_port = backend_addr.port();

    // 2. Create a temporary config file with a default route
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("test_default_config.toml");
    let config_content = format!(
        r#" 
health_check_interval = 1

[default_route]
backend = "http://127.0.0.1:{}"
"#,
        backend_port
    );
    let mut file = fs::File::create(&config_path).unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    let config = echoserver::config::load_config(config_path.to_str().unwrap()).unwrap();

    // 3. Start the proxy application with the temporary config
    let (proxy_addr, proxy_handle) = spawn_app_with_config(config).await;

    // 4. Send a request to the proxy with a non-matching host
    let client = Client::new();
    let proxy_url = format!("http://{}/{}", proxy_addr, "default_path");
    let response = client.get(&proxy_url)
        .header("Host", "nonexistent.localhost") // This host should not match any specific route
        .send()
        .await
        .expect("Failed to send request to proxy");

    // 5. Verify the response
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("Failed to get response body");
    assert_eq!(body, "Mock Backend Response for /default_path");

    // Clean up
    backend_handle.abort();
    proxy_handle.abort();
    temp_dir.close().unwrap();
}

#[tokio::test]
async fn test_static_file_serving() {
    // 1. Create a temporary directory and a dummy static file
    let temp_dir = tempfile::tempdir().unwrap();
    let static_root = temp_dir.path().join("static");
    fs::create_dir(&static_root).unwrap();
    let test_file_path = static_root.join("test.txt");
    fs::write(&test_file_path, "Hello from static file!").unwrap();

    // 2. Create a temporary config file with a static_files entry
    let config_path = temp_dir.path().join("test_static_config.toml");
    let config_content = format!(
        r#" 
health_check_interval = 1

[[static_files]]
path = "/static/"
root = "{}"
"#,
        static_root.to_string_lossy() // Ensure path is correct for TOML
    );
    let mut file = fs::File::create(&config_path).unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    let config = echoserver::config::load_config(config_path.to_str().unwrap()).unwrap();

    // 3. Start the proxy application with the temporary config
    let (proxy_addr, proxy_handle) = spawn_app_with_config(config).await;

    // 4. Send a request to the proxy for the static file
    let client = Client::new();
    let proxy_url = format!("http://{}/{}", proxy_addr, "static/test.txt");
    let response = client.get(&proxy_url)
        .send()
        .await
        .expect("Failed to send request to proxy");

    // 5. Verify the response
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "text/plain");
    let body = response.text().await.expect("Failed to get response body");
    assert_eq!(body, "Hello from static file!");

    // Clean up
    proxy_handle.abort();
    temp_dir.close().unwrap();
}

#[tokio::test]
async fn test_logging_middleware() {
    // 1. Start a mock backend server
    let (backend_addr, backend_handle) = mock_backend().await;
    let backend_port = backend_addr.port();

    // 2. Create a temporary config file with logging middleware enabled
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("test_middleware_config.toml");
    let config_content = format!(
        r#"
health_check_interval = 1

[[routes]]
host = "middleware.localhost"
backends = ["http://127.0.0.1:{}"]

middlewares = ["logging"]
"#,
        backend_port
    );
    let mut file = fs::File::create(&config_path).unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    let config = echoserver::config::load_config(config_path.to_str().unwrap()).unwrap();

    // 3. Start the proxy application with the temporary config
    let (proxy_addr, proxy_handle) = spawn_app_with_config(config).await;

    // 4. Send a request to the proxy
    let client = Client::new();
    let proxy_url = format!("http://{}/{}", proxy_addr, "test_middleware_path");
    let response = client.get(&proxy_url)
        .header("Host", "middleware.localhost")
        .send()
        .await
        .expect("Failed to send request to proxy");

    // 5. Verify the response (should still proxy correctly)
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("Failed to get response body");
    assert_eq!(body, "Mock Backend Response for /test_middleware_path");

    // Clean up
    backend_handle.abort();
    proxy_handle.abort();
    temp_dir.close().unwrap();
}
