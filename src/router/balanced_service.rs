use crate::router::PortMap;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct BalancedService {
    pub selector: BTreeMap<String, String>,
    pub port_map: PortMap,
}

impl BalancedService {
    pub fn new(selector: BTreeMap<String, String>, port_map: PortMap) -> BalancedService {
        BalancedService {
            selector,
            port_map,
        }
    }
}
