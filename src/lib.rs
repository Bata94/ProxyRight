use std::env;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode, Client, Uri};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use hyper::server::conn::AddrStream;

pub mod config;
pub mod health;
pub mod middleware;
pub mod middlewares;

use config::{Config, HealthStatus};

async fn router_handler(req: Request<Body>, config: Arc<Config>) -> Result<Response<Body>, Infallible> {
    // Handle static file serving
    let request_host = req.headers().get("host").and_then(|h| h.to_str().ok());
    for static_route in &config.static_files {
        let host_matches = if let Some(static_host) = &static_route.host {
            request_host == Some(static_host.as_str())
        } else {
            true // If static_route.host is None, it matches all hosts
        };

        if host_matches && req.uri().path().starts_with(&static_route.path) {
            let file_path_suffix = &req.uri().path()[static_route.path.len()..];
            let full_path = format!("{}/{}", static_route.root, file_path_suffix);

            match tokio::fs::read(&full_path).await {
                Ok(content) => {
                    let mime_type = mime_guess::from_path(&full_path).first_or_octet_stream();
                    return Ok(Response::builder()
                        .header(hyper::header::CONTENT_TYPE, mime_type.as_ref())
                        .body(Body::from(content))
                        .unwrap());
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        // File not found, continue to next handler (routes or default)
                        continue;
                    } else {
                        // Other error (e.g., permission denied)
                        let mut response = Response::new(Body::from(format!("Internal Server Error: {}", e)));
                        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                        return Ok(response);
                    }
                }
            }
        }
    }

    let host = req.headers().get("host").and_then(|h| h.to_str().ok());

    if let Some(host) = host {
        for route_mutex in &config.routes {
            let route = route_mutex.lock().await;
            if host == route.host {
                let num_backends = route.backends.len();
                if num_backends == 0 {
                    let mut response = Response::new(Body::from("No backends configured"));
                    *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
                    return Ok(response);
                }

                let mut healthy_backend_found = false;
                let mut selected_backend_url = String::new();

                for _ in 0..num_backends {
                    let current_index = route.current_backend_index.fetch_add(1, Ordering::SeqCst) % num_backends;

                    if route.statuses[current_index] == HealthStatus::Healthy {
                        selected_backend_url = route.backends[current_index].clone();
                        healthy_backend_found = true;
                        break;
                    }
                }

                if !healthy_backend_found {
                    let mut response = Response::new(Body::from("Service Unavailable"));
                    *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
                    return Ok(response);
                }

                let client = Client::new();
                let (mut parts, body) = req.into_parts();

                let path_and_query = parts.uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
                let new_uri = format!("{}{}", selected_backend_url, path_and_query);

                parts.uri = new_uri.parse::<Uri>().unwrap();
                parts.headers.remove("host");

                let new_req = Request::from_parts(parts, body);

                return match client.request(new_req).await {
                    Ok(res) => Ok(res),
                    Err(_) => {
                        let mut response = Response::new(Body::from("Bad Gateway"));
                        *response.status_mut() = StatusCode::BAD_GATEWAY;
                        Ok(response)
                    }
                };
            }
        }
    }

    if let Some(default_route_mutex) = &config.default_route {
        let default_route = default_route_mutex.lock().await;
        let num_backends = default_route.backends.len();
        if num_backends == 0 {
            let mut response = Response::new(Body::from("No backends configured for default route"));
            *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            return Ok(response);
        }

        let mut healthy_backend_found = false;
        let mut selected_backend_url = String::new();

        for _ in 0..num_backends {
            let current_index = default_route.current_backend_index.fetch_add(1, Ordering::SeqCst) % num_backends;

            if default_route.statuses[current_index] == HealthStatus::Healthy {
                selected_backend_url = default_route.backends[current_index].clone();
                healthy_backend_found = true;
                break;
            }
        }

        if !healthy_backend_found {
            let mut response = Response::new(Body::from("Service Unavailable for default route"));
            *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            return Ok(response);
        }

        let client = Client::new();
        let (mut parts, body) = req.into_parts();

        let path_and_query = parts.uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
        let new_uri = format!("{}{}", selected_backend_url, path_and_query);

        parts.uri = new_uri.parse::<Uri>().unwrap();
        parts.headers.remove("host");

        let new_req = Request::from_parts(parts, body);

        return match client.request(new_req).await {
            Ok(res) => Ok(res),
            Err(_) => {
                let mut response = Response::new(Body::from("Bad Gateway for default route"));
                *response.status_mut() = StatusCode::BAD_GATEWAY;
                Ok(response)
            }
        };
    }

    // Return a 404 Not Found response if no route matches and no default route is configured
    let mut not_found = Response::default();
    *not_found.status_mut() = StatusCode::NOT_FOUND;
    Ok(not_found)
}

use crate::middleware::{Middleware, Next};
use crate::middlewares::{LoggingMiddleware, BasicAuthMiddleware, RateLimitingMiddleware}; // Import RateLimitingMiddleware

pub async fn handle_request(req: Request<Body>, config: Arc<Config>) -> Result<Response<Body>, Infallible> {
    let mut middlewares: Vec<Arc<dyn Middleware>> = Vec::new(); // Use Arc for middlewares

    // Add instantiated middlewares from config
    for middleware in &config.instantiated_middlewares {
        middlewares.push(middleware.clone());
    }

    let mut current_handler: Next = Next::new(move |req| {
        let config_clone = config.clone();
        Box::pin(async move {
            router_handler(req, config_clone).await
        })
    });

    for middleware in middlewares.into_iter().rev() { // Iterate in reverse
        let next_handler = current_handler.clone(); // Clone next_handler
        current_handler = Next::new(move |req| {
            let middleware_clone = middleware.clone(); // Clone Arc for each call
            middleware_clone.handle(req, next_handler.clone()) // Directly return the future
        });
    }

    current_handler.run(req).await
}

pub async fn run() {
    let config_path = env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
    let config = Arc::new(config::load_config(&config_path).expect("Failed to load config"));

    let health_check_interval = config.health_check_interval;
    health::health_check_scheduler(config.routes.clone(), health_check_interval);

    let addr = env::args().nth(1).unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let socket_addr: SocketAddr = addr.parse().expect("Invalid address");



    let make_svc = make_service_fn(move |conn: &AddrStream| {
        let config = config.clone();
        let remote_addr = conn.remote_addr();
        async move {
            Ok::<_, Infallible>(service_fn(move |mut req| {
                req.extensions_mut().insert(remote_addr);
                handle_request(req, config.clone())
            }))
        }
    });

    println!("Listening on {}", socket_addr);
    let server = Server::bind(&socket_addr).serve(make_svc);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
