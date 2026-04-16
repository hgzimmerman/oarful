//! Per-request tracing middleware. Generates an 8-byte hex request
//! ID, opens a tracing span with it, and returns it in the
//! `x-request-id` response header.

use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use tracing::Instrument;

/// Generate an 8-byte hex request ID from the system random state.
fn generate_request_id() -> String {
    let h = RandomState::new().build_hasher().finish();
    format!("{h:016x}")
}

/// Middleware that creates a tracing span per request with a unique
/// request ID, method, and path. The request ID is also returned in
/// the `x-request-id` response header for client-side correlation.
pub(crate) async fn request_tracing(req: Request, next: Next) -> Response {
    let request_id = generate_request_id();
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let span = tracing::debug_span!(
        "request",
        id = %request_id,
        method = %method,
        path = %path,
    );

    let mut response = async {
        tracing::info!("started");
        let response = next.run(req).await;
        tracing::info!(status = %response.status(), "finished");
        response
    }
    .instrument(span)
    .await;

    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", val);
    }

    response
}
