//! Structured audit log for gRPC management API (`target: skadi.grpc.audit`).

use tonic::Request;

pub fn peer_addr<T>(req: &Request<T>) -> Option<String> {
    req.remote_addr().map(|a| a.to_string())
}

pub fn audit_auth_failure(peer: Option<&str>, reason: &str) {
    tracing::warn!(
        target: "skadi.grpc.audit",
        peer = peer.unwrap_or("unknown"),
        reason,
        "grpc auth failure"
    );
}

pub fn audit_rate_limited(peer: Option<&str>) {
    tracing::warn!(
        target: "skadi.grpc.audit",
        peer = peer.unwrap_or("unknown"),
        "grpc rate limited"
    );
}

pub fn audit_rpc(peer: Option<&str>, method: &str, detail: &str, ok: bool) {
    tracing::info!(
        target: "skadi.grpc.audit",
        peer = peer.unwrap_or("unknown"),
        method,
        ok,
        detail,
        "grpc rpc"
    );
}
