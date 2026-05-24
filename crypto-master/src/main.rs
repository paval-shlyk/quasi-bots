mod grpc;
mod performance;
mod rest;
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

    let rest_addr: std::net::SocketAddr = std::env::var("MASTER_REST_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;

    let pool = Arc::new(WorkerPool::new());
    pool.discover_from_env().await?;

    let worker_count = pool.list_ids().await.len();
    tracing::info!(workers = worker_count, "Worker pool initialized");

    let master = MasterGrpcServer::new(Arc::clone(&pool));

    let grpc_handle = tokio::spawn(async move {
        tracing::info!(%grpc_addr, "MasterService gRPC listening");
        Server::builder()
            .add_service(MasterServiceServer::new(master))
            .serve(grpc_addr)
            .await
    });

    let rest_app = rest::router(Arc::clone(&pool));
    let rest_listener = tokio::net::TcpListener::bind(rest_addr).await?;
    let rest_handle = tokio::spawn(async move {
        tracing::info!(%rest_addr, "REST API listening");
        axum::serve(rest_listener, rest_app).await
    });

    tokio::select! {
        res = grpc_handle => {
            if let Err(e) = res {
                tracing::error!(error = %e, "gRPC server task failed");
            }
        }
        res = rest_handle => {
            if let Err(e) = res {
                tracing::error!(error = %e, "REST server task failed");
            }
        }
    }

    Ok(())
}
