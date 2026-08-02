use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;

/// Middleware that measures request duration and reports to telemetry crate.
pub async fn track_http(req: Request, next: Next) -> Response {
    let start = Instant::now();

    let method = req.method().to_string();
    let uri = req.uri().path().to_string();

    let resp = next.run(req).await;

    let elapsed = start.elapsed().as_millis() as f64;
    let status = resp.status().as_u16();

    let histogram = telemetry::histogram!("http_request_duration_millis", "path" => uri.clone(), "method" => method.clone(), "status" => status.to_string());
    histogram.record(elapsed);

    let counter = telemetry::counter!("http_requests_total","path" => uri, "method" => method, "status" => status.to_string());
    counter.increment(1);

    resp
}
