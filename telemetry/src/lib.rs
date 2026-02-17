pub use metrics::{
    counter, describe_counter, describe_gauge, describe_histogram, gauge,
    histogram,
};

pub use metrics_exporter_prometheus::PrometheusHandle;

use std::time::Instant;

pub struct ExecutionTime {
    start: Instant,
    name: &'static str,
}

impl ExecutionTime {
    pub fn new(name: &'static str) -> Self {
        Self {
            start: Instant::now(),
            name,
        }
    }
}

impl Drop for ExecutionTime {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed().as_secs_f64();

        let histogram = metrics::histogram!("function_execution_ms", "function" => self.name);

        histogram.record(elapsed);
    }
}

#[macro_export]
macro_rules! execution_time {
    ($name:expr) => {
        let _timer = $crate::ExecutionTime::new($name);
    };
}

pub fn init_prometheus_recorder()
-> metrics_exporter_prometheus::PrometheusHandle {
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    builder
        .install_recorder()
        .expect("failed to install Prometheus recorder")
}

/// # Arguments
/// - epoch -- duration in seconds between metric measurement
pub fn spawn_system_monitor(epoch: u64) {
    tokio::task::spawn(async move {
        let gauge = metrics::gauge!("memory_used");

        loop {
            if let Some(usage) = memory_stats::memory_stats() {
                gauge.set(usage.physical_mem as f64);
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(epoch)).await;
        }
    });
}
