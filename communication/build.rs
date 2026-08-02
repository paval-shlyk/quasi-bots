fn main() -> Result<(), Box<dyn std::error::Error>> {
    let timestamp_serde =
        "#[serde(with = \"crate::serde_timestamp\", default)]";

    let timestamp_fields = [
        ".crypto.WorkerStatus.started_at",
        ".crypto.WorkerStatus.last_heartbeat",
        ".crypto.OpenPosition.opened_at",
        ".crypto.OpenPosition.updated_at",
        ".crypto.TradeRecord.created_at",
        ".crypto.TradeRecord.updated_at",
        ".crypto.OpenPrediction.opened_at",
        ".crypto.OpenPrediction.updated_at",
        ".crypto.PredictionRecord.created_at",
        ".crypto.PredictionRecord.updated_at",
        ".crypto.PerformanceReport.period_start",
        ".crypto.PerformanceReport.period_end",
    ];

    let mut config = tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");

    for field in &timestamp_fields {
        config = config.field_attribute(field, timestamp_serde);
    }

    config.compile_protos(&["proto/crypto.proto"], &["proto"])?;
    Ok(())
}
