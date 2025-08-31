use hyper::{Body, Request, Response};
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub trait Middleware: Send + Sync + 'static {
    fn handle(
        &self, // No lifetime 'a here
        req: Request<Body>,
        next: Next, // No lifetime 'a here
    ) -> Pin<Box<dyn Future<Output = Result<Response<Body>, Infallible>> + Send>>;
}

#[derive(Clone)]
pub struct Next { // No lifetime 'a here
    inner: Arc<dyn Fn(Request<Body>) -> Pin<Box<dyn Future<Output = Result<Response<Body>, Infallible>> + Send>> + Send + Sync + 'static>, // 'static lifetime
}

impl Next { // No lifetime 'a here
    pub fn new<F, Fut>(f: F) -> Self
    where
        F: Fn(Request<Body>) -> Fut + Send + Sync + 'static, // 'static lifetime
        Fut: Future<Output = Result<Response<Body>, Infallible>> + Send + 'static, // 'static lifetime
    {
        Next {
            inner: Arc::new(move |req| Box::pin(f(req))),
        }
    }

    pub fn run(
        self,
        req: Request<Body>,
    ) -> Pin<Box<dyn Future<Output = Result<Response<Body>, Infallible>> + Send>> { // No lifetime 'a here
        (self.inner)(req)
    }
}
