use crate::router::{Router, BackendPod};
use crate::{Result, NodeBalancerError};
use kube::Api;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use std::collections::{BTreeMap, HashMap};
use kube_runtime::watcher;
use futures_util::TryStreamExt;
use kube_runtime::watcher::Event;
use log::{info, warn};

impl Router {
    pub async fn fetch_pods(&self, selector: &BTreeMap<String, String>) -> Result<HashMap<String, BackendPod>> {
        let pod_api: Api<Pod> = Api::all(self.client.clone());

        let params = ListParams::default()
            .labels(Self::build_selector(selector).as_str());

        let pods = pod_api.list(&params).await.map_err(NodeBalancerError::KubeError)?.items;

        // pod_name -> node_name
        Ok(self.map_pods(pods))
    }

    fn map_pods(&self, pods: Vec<Pod>) -> HashMap<String, BackendPod> {
        pods.into_iter()
            .filter_map(|pod| pod.metadata.name.clone().map(|name| (name, pod)))
            .map(|(name, pod)| {
                let matches = self.svc_matches(&pod);
                (name, pod, matches)
            })
            .filter_map(|(name, pod, svc_matches)| pod.spec.map(|spec| (name, spec, svc_matches)))
            .filter_map(|(name, spec, svc_matches)| spec.node_name.map(|node_name| (name, BackendPod::new(node_name, svc_matches))))
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
                    self.map_pods(vec![pod]).into_iter()
                        .for_each(|(name, backend_pod)| {
                            let pod_names = self.pod_names.write();

                            info!("Got new pod {}", name);
                            self.pods.write().insert(name, backend_pod);

                            // See comments in get_destination
                            drop(pod_names);
                        });
                }

                Event::Deleted(pod) => {
                    if let Some(name) = pod.metadata.name {
                        self.pod_names.write().retain(|pod_name| pod_name != &name);
                        self.pods.write().remove(&name);
                        info!("Deleted pod {}", name);
                    }
                }

                Event::Restarted(_pods) => {
                    info!("Got pod stream restarted");
                }
            }

            Ok(())
        }).await.map_err(NodeBalancerError::WatcherError)?;

        Ok(())
    }

    fn svc_matches(&self, pod: &Pod) -> bool {
        match &*self.service.read() {
            Some(svc) => {
                for (key, value) in &svc.selector {
                    match pod.metadata.labels.get(key) {
                        Some(v) => {
                            if value != v {
                                return false;
                            }
                        }
                        None => {
                            return false;
                        }
                    }
                }

                true
            }
            None => {
                warn!("Service is None");
                false
            }
        }
    }

    fn build_selector(selector: &BTreeMap<String, String>) -> String {
        selector.iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<String>>()
            .join(",")
    }
}