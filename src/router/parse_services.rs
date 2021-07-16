use crate::router::{Router, PortMap, BalancedService};
use crate::{Result, NodeBalancerError};
use kube::Api;
use k8s_openapi::api::core::v1::{Service, ServicePort};
use kube::api::ListParams;
use std::collections::HashMap;
use kube_runtime::watcher;
use futures_util::TryStreamExt;
use kube_runtime::watcher::Event;
use log::info;

impl Router {
    pub async fn fetch_services(&self) -> Result<HashMap<String, BalancedService>> {
        let svc_api: Api<Service> = Api::all(self.client.clone());

        let params = ListParams::default();
        let services = svc_api.list(&params).await.map_err(NodeBalancerError::KubeError)?.items;
        Ok(Self::map_services(services))
    }

    fn map_services(services: Vec<Service>) -> HashMap<String, BalancedService> {
        services.into_iter()
            .filter(|svc| svc.metadata.annotations.contains_key("ryan.gdn/node-balancer"))
            .filter_map(|svc| svc.metadata.name.clone().map(|name| (svc, name)))
            .filter_map(|(svc, name)| svc.spec.map(|spec| (spec, name)))
            .filter(|(svc, _)| svc.type_.as_deref() == Some("NodePort"))
            .map(|(svc, name)| (name, BalancedService::new(svc.selector, Self::parse_port_map(svc.ports))))
            .collect()
    }

    pub async fn watch_services(&self) -> Result<()> {
        let svc_api: Api<Service> = Api::all(self.client.clone());
        let params = ListParams::default();

        let watcher = watcher(svc_api, params);
        watcher.try_for_each(|ev| async {
            match ev {
                // Update or delete
                Event::Applied(svc) => {
                    Self::map_services(vec![svc]).into_iter()
                        .for_each(|(name, balanced_service)| {
                            info!("Got new service {} with port map {:?}", name, balanced_service.port_map);
                            self.services.write().insert(name, balanced_service);
                        });
                }

                Event::Deleted(svc) => {
                    if let Some(name) = svc.metadata.name {
                        self.services.write().remove(&name);
                        info!("Deleted service {}", name);
                    }
                }

                Event::Restarted(services) => {
                    info!("Got service stream restarted");

                    let services = Self::map_services(services);
                    *self.services.write() = services;
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