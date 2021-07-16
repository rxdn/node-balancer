use thiserror::Error;

pub type Result<T> = std::result::Result<T, NodeBalancerError>;

#[derive(Error, Debug)]
pub enum NodeBalancerError {
    #[error("error operating on k8s: {0}")]
    KubeError(kube::Error),

    #[error("no pods available")]
    NoPodsAvailable,

    #[error("unknown node: {0}")]
    UnknownNode(String),

    #[error("node {0} has no addresses")]
    NoAddressesAvailable(String),

    #[error("service {0} not found")]
    UnknownService(String),

    #[error("port {0} not found for svc {1}")]
    UnknownPort(u16, String),

    #[error("kube watcher returned an error: {0}")]
    WatcherError(#[from] kube_runtime::watcher::Error),
}

impl<T> Into<Result<T>> for NodeBalancerError {
    fn into(self) -> Result<T> {
        Err(self)
    }
}
