#[derive(Clone, Debug)]
pub struct BackendPod {
    pub node: String,
    pub is_service_backend: bool,
}

impl BackendPod {
    pub fn new(node: String, is_service_backend: bool) -> BackendPod {
        BackendPod {
            node,
            is_service_backend,
        }
    }
}
