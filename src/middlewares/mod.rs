pub mod basic_auth;
pub mod rate_limiting;
pub use basic_auth::BasicAuthMiddleware;
pub use rate_limiting::RateLimitingMiddleware;

use crate::middleware::{Middleware, Next};
use hyper::{Body, Request, Response};
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use chrono::Local; // Import Local for local time
use std::time::Instant; // Import Instant for measuring duration

pub struct LoggingMiddleware;

impl Middleware for LoggingMiddleware {
    fn handle(
        &self,
        req: Request<Body>,
        next: Next,
    ) -> Pin<Box<dyn Future<Output = Result<Response<Body>, Infallible>> + Send>> {
        Box::pin(async move {
            let start_time = Instant::now();
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

            // Log incoming request details
            println!("[{}] Incoming Request: {} {}", timestamp, req.method(), req.uri());
            for (name, value) in req.headers() {
                println!("  {}: {}", name, value.to_str().unwrap_or("[unprintable]"));
            }

            let response = next.run(req).await;

            // Log outgoing response details
            let duration = start_time.elapsed();
            if let Ok(res) = &response {
                println!("[{}] Outgoing Response: {} ({}ms)", timestamp, res.status(), duration.as_millis());
                for (name, value) in res.headers() {
                    println!("  {}: {}", name, value.to_str().unwrap_or("[unprintable]"));
                }
            } else if let Err(e) = &response {
                eprintln!("[{}] Request Error: {} ({}ms)", timestamp, e, duration.as_millis());
            }
            response
        })
    }
}
