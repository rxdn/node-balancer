#[derive(Clone, Debug)]
pub struct BackendPod {
    pub node: String,
    pub associated_service: String,
}

impl BackendPod {
    pub fn new(node: String, associated_service: String) -> BackendPod {
        BackendPod {
            node,
            associated_service,
        }
    }
}
