use crate::middleware::{Middleware, Next};
use hyper::{Body, Request, Response, StatusCode};
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use governor::{Quota, RateLimiter};
use governor::state::keyed::DashMapStateStore;
use governor::clock::Clock;
use std::num::NonZeroU32;
use std::net::IpAddr;

pub struct RateLimitingMiddleware {
    limiter: Arc<RateLimiter<IpAddr, DashMapStateStore<IpAddr>, governor::clock::DefaultClock>>,
}

impl RateLimitingMiddleware {
    pub fn new(rps: u32, burst: u32) -> Self {
        let quota = Quota::per_second(NonZeroU32::new(rps).unwrap_or(NonZeroU32::new(1).unwrap()))
            .allow_burst(NonZeroU32::new(burst).unwrap_or(NonZeroU32::new(1).unwrap()));
        println!("RateLimitingMiddleware: Initializing with rps: {}, burst: {}, quota: {:?}", rps, burst, quota);
        let limiter = Arc::new(RateLimiter::keyed(quota));
        RateLimitingMiddleware { limiter }
    }
}

impl Middleware for RateLimitingMiddleware {
    fn handle(
        &self,
        req: Request<Body>,
        next: Next,
    ) -> Pin<Box<dyn Future<Output = Result<Response<Body>, Infallible>> + Send>> {
        let limiter = self.limiter.clone();
        Box::pin(async move {
            let remote_addr = req.extensions().get::<std::net::SocketAddr>().map(|s| s.ip());

            if let Some(ip_addr) = remote_addr {
                println!("RateLimitingMiddleware: Processing request from IP: {}", ip_addr);
                match limiter.check_key(&ip_addr) {
                    Ok(_) => {
                        println!("RateLimitingMiddleware: Request allowed for IP: {}", ip_addr);
                    },
                    Err(negative_limit) => {
                        println!("RateLimitingMiddleware: Request denied for IP: {}. Too many requests.", ip_addr);
                        let mut response = Response::new(Body::from("Too Many Requests"));
                        *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
                        // Optionally, add a Retry-After header
                        let wait_time = negative_limit.wait_time_from(governor::clock::DefaultClock::default().now());
                        response.headers_mut().insert(
                            hyper::header::RETRY_AFTER,
                            wait_time.as_secs().to_string().parse().unwrap(),
                        );
                        return Ok(response);
                    }
                }
            } else {
                println!("RateLimitingMiddleware: Could not determine remote IP address.");
            }

            next.run(req).await
        })
    }
}
