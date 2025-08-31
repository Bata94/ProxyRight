use crate::middleware::{Middleware, Next};
use hyper::{Body, Request, Response, StatusCode};
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use base64::{engine::general_purpose, Engine as _};

pub struct BasicAuthMiddleware {
    username: String,
    password: String,
}

impl BasicAuthMiddleware {
    pub fn new(username: String, password: String) -> Self {
        BasicAuthMiddleware { username, password }
    }
}

impl Middleware for BasicAuthMiddleware {
    fn handle(
        &self,
        req: Request<Body>,
        next: Next,
    ) -> Pin<Box<dyn Future<Output = Result<Response<Body>, Infallible>> + Send>> {
        let username = self.username.clone();
        let password = self.password.clone();

        Box::pin(async move {
            let auth_header = req.headers().get(hyper::header::AUTHORIZATION);

            if let Some(header_value) = auth_header {
                if let Ok(header_str) = header_value.to_str() {
                    if header_str.starts_with("Basic ") {
                        let encoded_credentials = &header_str[6..];
                        match general_purpose::STANDARD.decode(encoded_credentials) {
                            Ok(decoded_bytes) => {
                                if let Ok(decoded_str) = String::from_utf8(decoded_bytes) {
                                    let parts: Vec<&str> = decoded_str.splitn(2, ":").collect();
                                    if parts.len() == 2 && parts[0] == username && parts[1] == password {
                                        // Credentials are valid, proceed to the next middleware/handler
                                        return next.run(req).await;
                                    }
                                }
                            }
                            Err(_) => {
                                // Base64 decode error
                            }
                        }
                    }
                }
            }

            // If no Authorization header, or invalid credentials, return 401 Unauthorized
            let mut response = Response::new(Body::from("Unauthorized"));
            *response.status_mut() = StatusCode::UNAUTHORIZED;
            response.headers_mut().insert(
                hyper::header::WWW_AUTHENTICATE,
                "Basic realm=\"Restricted Area\"".parse().unwrap()
            );
            Ok(response)
        })
    }
}
