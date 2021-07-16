use crate::router::{Router, AddressableNode};
use crate::{Result, NodeBalancerError};
use kube::Api;
use k8s_openapi::api::core::v1::{Node, NodeStatus};
use kube::api::ListParams;
use kube_runtime::watcher;
use futures_util::TryStreamExt;
use kube_runtime::watcher::Event;
use std::collections::HashMap;
use log::info;

impl Router {
    pub async fn fetch_nodes(&self) -> Result<HashMap<String, AddressableNode>> {
        let node_api: Api<Node> = Api::all(self.client.clone());

        let params = ListParams::default();
        let nodes = node_api.list(&params).await.map_err(NodeBalancerError::KubeError)?.items;

        Ok(Self::map_nodes(nodes))
    }

    fn map_nodes(nodes: Vec<Node>) -> HashMap<String, AddressableNode> {
        nodes.into_iter()
            .filter_map(|node| node.metadata.name.clone().map(|name| (node, name)))
            .filter_map(|(node, name)| node.status.map(|status| (status, name)))
            .map(|(status, name)| (name, AddressableNode::new(Self::extract_addresses(status))))
            .filter(|(_, node)| !node.addresses.is_empty())
            .collect()
    }

    pub async fn watch_nodes(&self) -> Result<()> {
        let node_api: Api<Node> = Api::all(self.client.clone());
        let params = ListParams::default();

        let watcher = watcher(node_api, params);
        watcher.try_for_each(|ev| async {
            match ev {
                // Update or delete
                Event::Applied(node) => {
                    Self::map_nodes(vec![node]).into_iter()
                        .for_each(|(name, addressable_node)| {
                            info!("Got new node {} with addresses {:?}", name, addressable_node.addresses);
                            self.nodes.write().insert(name, addressable_node);
                        });
                }

                // TODO: Investigate whether this fires pod deletions too
                Event::Deleted(node) => {
                    if let Some(name) = node.metadata.name {
                        self.nodes.write().remove(&name);
                        info!("Deleted node {}", name);
                    }
                }

                Event::Restarted(nodes) => {
                    info!("Got node stream restarted");

                    let nodes = Self::map_nodes(nodes);
                    *self.nodes.write() = nodes;
                }
            }

            Ok(())
        }).await.map_err(NodeBalancerError::WatcherError)?;

        Ok(())
    }

    fn extract_addresses(status: NodeStatus) -> Vec<String> {
        status.addresses.into_iter()
            .filter(|address| address.type_ == "InternalIP")
            .map(|address| address.address)
            .collect()
    }
}