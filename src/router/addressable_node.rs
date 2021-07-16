#[derive(Clone, Debug)]
pub struct AddressableNode {
    pub addresses: Vec<String>,
}

impl AddressableNode {
    pub fn new(addresses: Vec<String>) -> AddressableNode {
        AddressableNode {
            addresses,
        }
    }
}
