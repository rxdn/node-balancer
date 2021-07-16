use node_balancer::router::Router;
use std::sync::Arc;
use node_balancer::Config;

// IMPORTANT
// SET externalTrafficPolicy on service to cluster not local
// not anymore ignore this
// TODO: EXPO BACKOFF WHEN MINIKUBE IS DOWN

#[tokio::main]
async fn main() {
    env_logger::init();

    let config = Arc::new(Config::from_envvar());

    let router = Router::new(Arc::clone(&config)).await;
    let router = Arc::new(router);

    router.seed().await.expect("Seeding router failed");
    router.start_watchers(false).await; // Blocks
}
