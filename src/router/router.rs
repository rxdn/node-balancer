use kube::{Client, Config as KubeConfig};
use std::convert::TryFrom;
use crate::{Result, NodeBalancerError, Config};
use crate::router::{AddressableNode, BalancedService, BackendPod};
use parking_lot::RwLock;
use rand::seq::SliceRandom;
use std::collections::{HashMap, BTreeMap};
use log::{error, info};
use std::sync::Arc;

pub struct Router {
    pub config: Arc<Config>,
    pub(super) client: Client,
    // name -> node
    pub(super) nodes: RwLock<HashMap<String, AddressableNode>>,
    // name -> svc
    pub(super) service: RwLock<Option<BalancedService>>,
    // name -> pod
    pub(super) pods: RwLock<HashMap<String, BackendPod>>,
    pub(super) pod_names: RwLock<Vec<String>>,
}

impl Router {
    pub async fn new(config: Arc<Config>) -> Router {
        let kube_config = KubeConfig::infer().await.unwrap();
        let client = Client::try_from(kube_config).unwrap();

        Router {
            config,
            client,
            nodes: RwLock::new(HashMap::new()),
            service: RwLock::new(None),
            pods: RwLock::new(HashMap::new()),
            pod_names: RwLock::new(Vec::new()),
        }
    }

    // TODO: Filter services by annotation name
    pub fn get_destination(&self, port: u16) -> Result<(String, u16)> {
        // Race condition shouldn't occur as we hold a read lock on pod_names until the end
        // (enforced by the drop). Just make sure to get a write lock on pod_names before touching pods
        let pod_names = self.pod_names.read();
        let pod_name = pod_names.choose(&mut rand::thread_rng()).ok_or(NodeBalancerError::NoPodsAvailable)?;

        let pods = self.pods.read();
        let pod = pods.get(pod_name).ok_or(NodeBalancerError::NoPodsAvailable)?;
        drop(pod_names);

        // Get node IP
        let nodes = self.nodes.read();
        let node_ips = &nodes.get(&pod.node).ok_or_else(|| NodeBalancerError::UnknownNode(pod.node.clone()))?.addresses;
        let ip = node_ips.choose(&mut rand::thread_rng()).ok_or_else(|| NodeBalancerError::NoAddressesAvailable(pod.node.clone()))?;

        // Get service port
        // Rust is dumb
        let lock = self.service.read();
        let dest_port = lock.as_ref()
            .ok_or(NodeBalancerError::ServiceNotFound)?
            .port_map
            .get(&port)
            .ok_or_else(|| NodeBalancerError::UnknownPort(port))?;

        Ok((ip.clone(), *dest_port))
    }

    pub async fn seed(&self) -> Result<()> {
        self.seed_nodes().await?;
        self.seed_service().await?;
        self.seed_pods().await?;

        Ok(())
    }

    pub async fn start_watchers(self: Arc<Self>, daemon: bool) {
        let node_handle = tokio::spawn({
            let router = Arc::clone(&self);

            async move {
                loop {
                    if let Err(e) = router.clone().watch_nodes().await {
                        error!("Error returned by node watcher: {}", e);
                    }

                    info!("Restarting node watcher");
                }
            }
        });

        let svc_handle = tokio::spawn({
            let router = Arc::clone(&self);

            async move {
                loop {
                    if let Err(e) = router.clone().watch_services().await {
                        error!("Error returned by service watcher: {}", e);
                    }

                    info!("Restarting service watcher");
                }
            }
        });

        let pod_handle = tokio::spawn({
            let router = Arc::clone(&self);

            async move {
                loop {
                    if let Err(e) = router.clone().watch_pods().await {
                        error!("Error returned by pod watcher: {}", e);
                    }

                    info!("Restarting pod watcher");
                }
            }
        });

        #[allow(unused_must_use)]
        if !daemon {
            // Will never return
            tokio::join!(node_handle, svc_handle, pod_handle);
        }
    }

    async fn seed_nodes(&self) -> Result<()> {
        let nodes = self.fetch_nodes().await?;
        *self.nodes.write() = nodes;
        Ok(())
    }

    async fn seed_service(&self) -> Result<()> {
        let service = self.fetch_service().await?;
        *self.service.write() = Some(service);
        Ok(())
    }

    pub(super) async fn seed_pods(&self) -> Result<()> {
        // Rust compiler is dumb
        let selector: BTreeMap<String, String>;
        {
            selector = self.service.read().clone().expect("cumzone").selector;
        }

        // Ok to panic, only run on startup and after successful service replacement
        let fetched_pods = self.fetch_pods(&selector).await?;

        let pod_names = &mut self.pod_names.write();
        let pods = &mut self.pods.write();

        pods.clear();
        pod_names.clear();

        fetched_pods.into_iter().for_each(|(pod_name, pod)| {
            pods.insert(pod_name.clone(), pod);
            pod_names.push(pod_name);
        });

        Ok(())
    }
}