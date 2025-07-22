use http_body_util::{BodyExt, Full};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, Uri};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::time::Duration;
use tokio::net::TcpListener;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

lazy_static::lazy_static! {
    static ref CLIENT: Client<HttpConnector, Full<hyper::body::Bytes>> = {
        let executor = TokioExecutor::new();
        Client::builder(executor)
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(100)
            .http2_only(false)
            .build_http()
    };
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let proxy_address = "127.0.0.1:8080";
    let listener = TcpListener::bind(proxy_address).await?;
    println!("Reverse proxy running on http://{}", proxy_address);

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let target = "http://127.0.0.1:3000".to_string();

        tokio::task::spawn(async move {
            // Create and configure the builder in one expression
            let server_builder = {
                let mut builder = http1::Builder::new();
                builder
                    .header_read_timeout(Duration::from_secs(5))
                    .keep_alive(true)
                    .pipeline_flush(true)
            };

            let service = service_fn({
                let target = target.clone();
                move |req: Request<hyper::body::Incoming>| {
                    let target = target.clone();
                    async move { proxy_request(req, target).await }
                }
            });

            if let Err(err) = server_builder
                .serve_connection(io, service)
                .with_upgrades()
                .await
            {
                eprintln!("Connection error: {}", err);
            }
        });
    }
}

async fn proxy_request(
    req: Request<hyper::body::Incoming>,
    target: String,
) -> Result<Response<Full<hyper::body::Bytes>>, Box<dyn std::error::Error + Send + Sync + 'static>>
{
    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("");
    let target_uri = format!("{}{}", target, path).parse::<Uri>()?;

    let (parts, body) = req.into_parts();
    let body_bytes = body.collect().await?.to_bytes();

    let mut client_request = Request::builder()
        .method(parts.method)
        .uri(target_uri)
        .body(Full::new(body_bytes))?;

    *client_request.headers_mut() = parts.headers;

    let response = CLIENT.request(client_request).await?;
    let (parts, body) = response.into_parts();
    let body_bytes = body.collect().await?.to_bytes();

    Ok(Response::from_parts(parts, Full::new(body_bytes)))
}
