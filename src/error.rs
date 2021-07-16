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

    #[error("service not found")]
    ServiceNotFound,

    #[error("port {0} not found")]
    UnknownPort(u16),

    #[error("kube watcher returned an error: {0}")]
    WatcherError(#[from] kube_runtime::watcher::Error),

    #[error("resource is missing spec")]
    MissingSpec,

    #[error("service wanted NodePort, got {0}")]
    WrongServiceType(String),

    #[error("error occurred during IO operation: {0}")]
    IOError(std::io::Error),
}

impl<T> Into<Result<T>> for NodeBalancerError {
    fn into(self) -> Result<T> {
        Err(self)
    }
}
