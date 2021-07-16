use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub service_namespace: String,
    pub service_name: String,
    pub ports: Vec<u16>,
    pub listen_addr: String,
}

impl Config {
    pub fn from_envvar() -> Config {
        envy::from_env().unwrap()
    }
}
