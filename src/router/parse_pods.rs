use crate::router::{Router, BackendPod, BalancedService};
use crate::{Result, NodeBalancerError};
use kube::Api;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use std::collections::{BTreeMap, HashMap};
use kube_runtime::watcher;
use futures_util::TryStreamExt;
use kube_runtime::watcher::Event;
use log::{debug, error, info};

impl Router {
    pub async fn fetch_pods(&self, service_name: String, selector: &BTreeMap<String, String>) -> Result<HashMap<String, BackendPod>> {
        let pod_api: Api<Pod> = Api::all(self.client.clone());

        let params = ListParams::default()
            .labels(Self::build_selector(selector).as_str());

        let pods = pod_api.list(&params).await.map_err(NodeBalancerError::KubeError)?.items;

        // pod_name -> node_name
        Ok(Self::map_pods(pods, service_name))
    }

    fn map_pods(pods: Vec<Pod>, service_name: String) -> HashMap<String, BackendPod> {
        pods.into_iter()
            .filter_map(|pod| pod.metadata.name.clone().map(|name| (name, pod)))
            .filter_map(|(name, pod)| pod.spec.map(|spec| (name, spec)))
            .filter_map(|(name, spec)| spec.node_name.map(|node_name| (name, BackendPod::new(node_name, service_name.clone()))))
            .collect()
    }

    pub async fn watch_pods(&self) -> Result<()> {
        let svc_api: Api<Pod> = Api::all(self.client.clone());
        let params = ListParams::default();

        let watcher = watcher(svc_api, params);
        watcher.try_for_each(|ev| async {
            match ev {
                // Update or delete
                Event::Applied(pod) => {
                    // Get service name
                    let svc = self.find_svc_for_pod(&pod);
                    let service_name = match svc {
                        Some((name, _)) => name,
                        None => {
                            debug!("No service found for pod {:?} with labels {:?}", pod.metadata.name, pod.metadata.labels);
                            return Ok(());
                        }
                    };

                    Self::map_pods(vec![pod], service_name.to_string()).into_iter()
                        .for_each(|(name, backend_pod)| {
                            let pod_names = self.pod_names.write();

                            info!("Got new pod {} with service {}", name, service_name);
                            self.pods.write().insert(name, backend_pod);

                            // See comments in get_destination
                            drop(pod_names);
                        });
                }

                Event::Deleted(svc) => {
                    if let Some(name) = svc.metadata.name {
                        self.services.write().remove(&name);
                        info!("Deleted service {}", name);
                    }
                }

                Event::Restarted(pods) => {
                    info!("Got pod stream restarted");
                }
            }

            Ok(())
        }).await.map_err(NodeBalancerError::WatcherError)?;

        Ok(())
    }

    fn find_svc_for_pod(&self, pod: &Pod) -> Option<(String, BalancedService)> {
        self.services.read().iter()
            .find(|(_, svc)| {
                let mut ok = true;

                for (key, value) in &svc.selector {
                    match pod.metadata.labels.get(key) {
                        Some(v) => {
                            if value != v {
                                ok = false;
                                break;
                            }
                        }
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }

                ok
            }).map(|(name, svc)| (name.clone(), svc.clone()))
    }

    fn build_selector(selector: &BTreeMap<String, String>) -> String {
        selector.iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<String>>()
            .join(",")
    }
}