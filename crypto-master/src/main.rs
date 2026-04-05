mod grpc;
mod performance;
mod worker_pool;

use std::sync::Arc;

use communication::MasterServiceServer;
use tonic::transport::Server;

use crate::grpc::MasterGrpcServer;
use crate::worker_pool::WorkerPool;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let grpc_addr = std::env::var("MASTER_GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50050".into())
        .parse()?;

    let pool = Arc::new(WorkerPool::new());
    pool.discover_from_env().await?;

    let worker_count = pool.list_ids().await.len();
    tracing::info!(workers = worker_count, "Worker pool initialized");

    let master = MasterGrpcServer::new(Arc::clone(&pool));

    tracing::info!(%grpc_addr, "MasterService listening");
    Server::builder()
        .add_service(MasterServiceServer::new(master))
        .serve(grpc_addr)
        .await?;

    Ok(())
}
