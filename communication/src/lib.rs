pub mod proto {
    tonic::include_proto!("crypto");
}

pub use proto::master_service_client::MasterServiceClient;
pub use proto::master_service_server::{MasterService, MasterServiceServer};
pub use proto::worker_service_client::WorkerServiceClient;
pub use proto::worker_service_server::{WorkerService, WorkerServiceServer};
