use crate::router::{Router, PortMap, BalancedService};
use crate::{Result, NodeBalancerError};
use kube::Api;
use k8s_openapi::api::core::v1::{Service, ServicePort};
use kube::api::ListParams;
use kube_runtime::watcher;
use futures_util::TryStreamExt;
use kube_runtime::watcher::Event;
use log::{error, info};

impl Router {
    pub async fn fetch_service(&self) -> Result<BalancedService> {
        let svc_api: Api<Service> = Api::namespaced(self.client.clone(), &self.config.service_namespace[..]);

        let svc = svc_api.get(&self.config.service_name[..]).await.map_err(NodeBalancerError::KubeError)?;
        Self::map_service(svc)
    }

    fn map_service(svc: Service) -> Result<BalancedService> {
        let spec = svc.spec.ok_or(NodeBalancerError::MissingSpec)?;
        if spec.type_.as_deref() != Some("NodePort") {
            return NodeBalancerError::WrongServiceType(spec.type_.as_deref().unwrap_or("None").to_owned()).into();
        }

        Ok(BalancedService::new(spec.selector, Self::parse_port_map(spec.ports)))
    }

    pub async fn watch_services(&self) -> Result<()> {
        let svc_api: Api<Service> = Api::all(self.client.clone());
        let params = ListParams::default();

        let watcher = watcher(svc_api, params);
        watcher.try_for_each(|ev| async {
            match ev {
                // Update or delete
                Event::Applied(svc) => {
                    let name = match svc.metadata.name.clone() {
                        Some(v) => v,
                        None => return Ok(()),
                    };

                    let namespace = match svc.metadata.namespace.clone() {
                        Some(v) => v,
                        None => return Ok(()),
                    };

                    if name == self.config.service_name && namespace == self.config.service_namespace {
                        match Self::map_service(svc) {
                            Ok(svc) => {
                                info!("Service {} registered with port map {:?}", self.config.service_name, svc.port_map);
                                *self.service.write() = Some(svc);

                                // Reload pods
                                if let Err(e) = self.seed_pods().await {
                                    error!("Error while re-seeding pods: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("Error while registering service: {}", e);
                                *self.service.write() = None;
                            }
                        }
                    }
                }

                Event::Deleted(svc) => {
                    if let Some(name) = svc.metadata.name {
                        if let Some(namespace) = svc.metadata.namespace {
                            if name == self.config.service_name && namespace == self.config.service_namespace {
                                *self.service.write() = None
                            }
                        }
                    }
                }

                Event::Restarted(_services) => {
                    info!("Got service stream restarted");
                    // TODO
                }
            }

            Ok(())
        }).await.map_err(NodeBalancerError::WatcherError)?;

        Ok(())
    }

    fn parse_port_map(ports: Vec<ServicePort>) -> PortMap {
        ports.iter()
            .filter_map(|port| port.node_port.map(|node_port| (port.port as u16, node_port as u16)))
            .collect()
    }
}