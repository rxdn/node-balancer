use node_balancer::router::Router;
use std::sync::Arc;

// IMPORTANT
// SET externalTrafficPolicy on service to cluster not local
// not anymore ignore this

#[tokio::main]
async fn main() {
    env_logger::init();

    let router = Arc::new(Router::new().await);
    router.seed().await.expect("Seeding router failed");
    router.start_watchers(false).await; // Blocks
}
