use crate::supervisor::ServiceManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<ServiceManager>,
    pub token: String,
}

impl AppState {
    pub fn new(manager: Arc<ServiceManager>, token: String) -> Self {
        Self { manager, token }
    }
}
