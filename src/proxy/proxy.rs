use crate::{Config, Result, NodeBalancerError};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io;
use tokio::io::AsyncWriteExt;
use crate::router::Router;

pub struct Proxy {
    pub config: Arc<Config>,
    pub router: Arc<Router>,
}

impl Proxy {
    pub fn new(config: Arc<Config>, router: Arc<Router>) -> Proxy {
        Proxy {
            config,
            router,
        }
    }

    pub async fn listen(self: Arc<Self>) {
        for port in self.config.ports {
            let proxy = Arc::clone(&self);
            tokio::spawn(async move {
                proxy.start_listener(port).await;
            });
        }
    }

    async fn start_listener(&self, port: u16) {
        loop {
            let listener = TcpListener::bind(format!("{}:{}", self.config.listen_addr, port)).await.expect("tcp listener failed to bind");

            while let Ok((inbound, _)) = listener.accept().await {
                let (dest_addr, dest_port) = self.router.get_destination(port).unwrap();

                tokio::spawn(async move {
                    if let Err(e) = Self::proxy(inbound, format!("{}:{}", dest_addr, dest_port)).await {
                        panic!("{}", e);
                    }
                });
            }
        }
    }

    async fn proxy(mut inbound: TcpStream, proxy_addr: String) -> Result<()> {
        let mut outbound = TcpStream::connect(proxy_addr).await.map_err(NodeBalancerError::IOError)?;

        let (mut ri, mut wi) = inbound.split();
        let (mut ro, mut wo) = outbound.split();

        let client_to_server = async {
            io::copy(&mut ri, &mut wo).await.map_err(NodeBalancerError::IOError)?;
            wo.shutdown().await
        };

        let server_to_client = async {
            io::copy(&mut ro, &mut wi).await.map_err(NodeBalancerError::IOError)?;
            wi.shutdown().await
        };

        tokio::try_join!(client_to_server, server_to_client).map_err(NodeBalancerError::IOError)?;

        Ok(())
    }
}