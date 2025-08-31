use crate::config::{Route, HealthStatus};
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;
use tokio::time::sleep;

pub async fn check_health(client: &Client, route: Arc<Mutex<Route>>, index: usize) {
    let route_clone = route.clone();
    let url = {
        let route_lock = route_clone.lock().await;
        route_lock.health_check_urls.as_ref().and_then(|urls| urls.get(index).cloned()).unwrap_or_default()
    };

    if url.is_empty() {
        let mut route_lock = route.lock().await;
        if let Some(status) = route_lock.statuses.get_mut(index) {
            *status = HealthStatus::Healthy; // Or Unhealthy, depending on desired default behavior
        }
        return;
    }

    let status = if let Ok(res) = client.get(&url).send().await {
        if res.status().is_success() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        }
    } else {
        HealthStatus::Unhealthy
    };

    let mut route_lock = route.lock().await;
    if let Some(s) = route_lock.statuses.get_mut(index) {
        *s = status;
    }
}

pub fn health_check_scheduler(routes: Vec<Arc<Mutex<Route>>>, interval: u64) {
    tokio::spawn(async move {
        let client = Client::new();
        loop {
            for route in &routes {
                let route_clone = route.clone();
                let client_clone = client.clone();
                let num_backends = {
                    let route_lock = route_clone.lock().await;
                    route_lock.backends.len()
                };

                for i in 0..num_backends {
                    let route_clone_inner = route_clone.clone();
                    let client_clone_inner = client_clone.clone();
                    tokio::spawn(async move {
                        check_health(&client_clone_inner, route_clone_inner, i).await;
                    });
                }
            }
            sleep(Duration::from_secs(interval)).await;
        }
    });
}