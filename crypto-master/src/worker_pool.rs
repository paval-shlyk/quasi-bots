use std::collections::HashMap;

use communication::WorkerServiceClient;
use tokio::sync::RwLock;
use tonic::transport::Channel;

/// A single discovered worker with its gRPC client and cached status.
pub struct WorkerConnection {
    pub id: String,
    pub address: String,
    pub client: WorkerServiceClient<Channel>,
}

impl WorkerConnection {
    pub async fn connect(id: String, address: String) -> anyhow::Result<Self> {
        let client = WorkerServiceClient::connect(address.clone()).await?;
        Ok(Self {
            id,
            address,
            client,
        })
    }
}

/// Pool of worker connections discovered from env at startup.
/// Thread-safe for concurrent gRPC calls from the MasterService.
pub struct WorkerPool {
    workers: RwLock<HashMap<String, WorkerConnection>>,
}

impl WorkerPool {
    pub fn new() -> Self {
        Self {
            workers: RwLock::new(HashMap::new()),
        }
    }

    /// Parse WORKER_ADDRESSES env: "id1=host:port,id2=host:port,..."
    pub async fn discover_from_env(&self) -> anyhow::Result<()> {
        let raw = std::env::var("WORKER_ADDRESSES").unwrap_or_default();
        if raw.is_empty() {
            tracing::warn!(
                "WORKER_ADDRESSES not set, no workers will be registered"
            );
            return Ok(());
        }

        for entry in raw.split(',') {
            let entry = entry.trim();
            let Some((id, addr)) = entry.split_once('=') else {
                tracing::warn!(
                    entry,
                    "Skipping malformed worker entry (expected id=host:port)"
                );
                continue;
            };

            let endpoint = if addr.starts_with("http") {
                addr.to_string()
            } else {
                format!("http://{addr}")
            };

            match WorkerConnection::connect(id.into(), endpoint.clone()).await {
                Ok(conn) => {
                    tracing::info!(worker_id = id, addr = %endpoint, "Worker connected");
                    self.workers.write().await.insert(id.into(), conn);
                }
                Err(e) => {
                    tracing::error!(worker_id = id, addr = %endpoint, error = %e, "Failed to connect to worker");
                }
            }
        }

        Ok(())
    }

    pub async fn list_ids(&self) -> Vec<String> {
        self.workers.read().await.keys().cloned().collect()
    }

    /// Run a closure against a specific worker's gRPC client.
    /// Returns NotFound if the worker_id is unknown.
    pub async fn with_worker<F, Fut, T>(
        &self,
        worker_id: &str,
        f: F,
    ) -> Result<T, tonic::Status>
    where
        F: FnOnce(WorkerServiceClient<Channel>) -> Fut,
        Fut: std::future::Future<Output = Result<T, tonic::Status>>,
    {
        let guard = self.workers.read().await;
        let conn = guard.get(worker_id).ok_or_else(|| {
            tonic::Status::not_found(format!("Unknown worker: {worker_id}"))
        })?;

        // Clone the client (tonic clients are cheap clones backed by a shared channel)
        let client = conn.client.clone();
        drop(guard);

        f(client).await
    }

    /// Run a closure against ALL workers, collecting results keyed by worker_id.
    pub async fn for_each<F, Fut, T>(
        &self,
        f: F,
    ) -> Vec<(String, Result<T, tonic::Status>)>
    where
        F: Fn(WorkerServiceClient<Channel>) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<T, tonic::Status>>
            + Send
            + 'static,
        T: Send + 'static,
    {
        let guard = self.workers.read().await;
        let mut handles = Vec::with_capacity(guard.len());

        for (id, conn) in guard.iter() {
            let client = conn.client.clone();
            let id = id.clone();
            let fut = f(client);
            handles.push((id, tokio::spawn(fut)));
        }
        drop(guard);

        let mut results = Vec::with_capacity(handles.len());
        for (id, handle) in handles {
            let result = match handle.await {
                Ok(r) => r,
                Err(e) => Err(tonic::Status::internal(format!(
                    "Task panicked for {id}: {e}"
                ))),
            };
            results.push((id, result));
        }
        results
    }
}
