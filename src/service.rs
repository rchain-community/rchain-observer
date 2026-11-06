use std::{
    fmt::Write as _,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::model::{ObservationReport, VerdictClass};

pub struct CachedReport {
    pub report: ObservationReport,
    pub sampled: Instant,
}

#[derive(Clone)]
pub struct ApiState {
    pub snapshot: Arc<RwLock<CachedReport>>,
    pub stale_after: Duration,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics))
        .route("/v1/observation", get(observation))
        .with_state(state)
        .layer(middleware::from_fn(headers))
}

async fn headers(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    response
}

/// Shared fail-closed readiness policy for JSON health and Prometheus metrics.
fn evidence_state(
    report: &ObservationReport,
    age: Duration,
    stale_after: Duration,
) -> (bool, bool) {
    let stale = age >= stale_after;
    let complete = report.verdict.observed_nodes >= 2
        && report.verdict.comparable_nodes == report.verdict.observed_nodes
        && report.heights.reachable_nodes == report.verdict.observed_nodes;
    let ready = !stale
        && complete
        && matches!(
            report.verdict.class,
            VerdictClass::Agreement | VerdictClass::ProvenLag
        );
    (stale, ready)
}

async fn health_payload(state: &ApiState) -> (bool, Value) {
    let snapshot = state.snapshot.read().await;
    let report = &snapshot.report;
    let age = snapshot.sampled.elapsed();
    let (stale, ready) = evidence_state(report, age, state.stale_after);
    (
        ready,
        json!({
            "version": env!("CARGO_PKG_VERSION"),
        "status": if stale { "stale" } else if ready { "ok" } else { "degraded" },
            "ready": ready,
            "stale": stale,
            "snapshotAgeMs": age.as_millis().min(u128::from(u64::MAX)) as u64,
            "staleAfterMs": state.stale_after.as_millis().min(u128::from(u64::MAX)) as u64,
            "observedAtMs": report.observed_at_ms,
            "completedAtMs": report.completed_at_ms,
            "durationMs": report.duration_ms,
            "observedNodes": report.verdict.observed_nodes,
            "comparableNodes": report.verdict.comparable_nodes,
            "reachableNodes": report.heights.reachable_nodes,
            "verdict": report.verdict.class,
            "ancestryChecks": report.ancestry_checks.len(),
        }),
    )
}

/// Liveness always returns 200; the payload separately describes evidence quality.
async fn health(State(state): State<ApiState>) -> Json<Value> {
    Json(health_payload(&state).await.1)
}

/// Readiness requires fresh, complete, multi-node evidence without observed conflicts.
async fn ready(State(state): State<ApiState>) -> (StatusCode, Json<Value>) {
    let (ready, payload) = health_payload(&state).await;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(payload),
    )
}

/// Prometheus text exposition uses only bounded, static metric names and labels.
/// No node names, endpoints, block hashes, or other untrusted labels are exported.
async fn metrics(
    State(state): State<ApiState>,
) -> ([(header::HeaderName, HeaderValue); 1], String) {
    let snapshot = state.snapshot.read().await;
    let report = &snapshot.report;
    let age = snapshot.sampled.elapsed();
    let (stale, ready) = evidence_state(report, age, state.stale_after);
    let age_ms = age.as_millis().min(u128::from(u64::MAX)) as u64;
    let verified_blocks: u64 = report
        .ancestry_checks
        .iter()
        .map(|check| check.cryptographically_verified_blocks as u64)
        .sum();

    let gauges = [
        (
            "rho_observer_ready",
            "1 when cached evidence meets the observer readiness policy.",
            u64::from(ready),
        ),
        (
            "rho_observer_stale",
            "1 when the cached sample exceeds the monotonic freshness threshold.",
            u64::from(stale),
        ),
        (
            "rho_observer_snapshot_age_milliseconds",
            "Monotonic age since the start of the cached sampling window.",
            age_ms,
        ),
        (
            "rho_observer_stale_after_milliseconds",
            "Configured threshold for stale cached evidence.",
            state.stale_after.as_millis().min(u128::from(u64::MAX)) as u64,
        ),
        (
            "rho_observer_sampling_duration_milliseconds",
            "Elapsed time to construct the latest observation report.",
            report.duration_ms,
        ),
        (
            "rho_observer_observed_nodes",
            "Number of configured node rows in the latest report.",
            report.verdict.observed_nodes as u64,
        ),
        (
            "rho_observer_reachable_nodes",
            "Number of endpoints with reachable snapshot responses.",
            report.heights.reachable_nodes as u64,
        ),
        (
            "rho_observer_comparable_nodes",
            "Number of nodes with complete comparable finalized identities.",
            report.verdict.comparable_nodes as u64,
        ),
        (
            "rho_observer_agreeing_nodes",
            "Number of comparable nodes supporting the selected reference.",
            report.verdict.agreeing_nodes as u64,
        ),
        (
            "rho_observer_ancestry_checks",
            "Number of bounded ancestry checks attempted in the latest report.",
            report.ancestry_checks.len() as u64,
        ),
        (
            "rho_observer_verified_ancestry_blocks",
            "Cryptographically verified raw blocks counted across ancestry checks.",
            verified_blocks,
        ),
    ];

    let mut body = String::with_capacity(2400);
    for (name, description, value) in gauges {
        writeln!(&mut body, "# HELP {name} {description}").expect("format metric help");
        writeln!(&mut body, "# TYPE {name} gauge").expect("format metric type");
        writeln!(&mut body, "{name} {value}").expect("format metric value");
    }

    body.push_str("# HELP rho_observer_verdict One-hot latest observer verdict (not a consensus certificate).\n");
    body.push_str("# TYPE rho_observer_verdict gauge\n");
    for (label, class) in [
        ("agreement", VerdictClass::Agreement),
        ("proven_lag", VerdictClass::ProvenLag),
        ("height_skew", VerdictClass::HeightSkew),
        ("majority", VerdictClass::Majority),
        (
            "conflicting_finalized_history",
            VerdictClass::ConflictingFinalizedHistory,
        ),
        ("divergence", VerdictClass::Divergence),
        ("unavailable", VerdictClass::Unavailable),
    ] {
        writeln!(
            &mut body,
            "rho_observer_verdict{{class=\"{label}\"}} {}",
            u64::from(report.verdict.class == class)
        )
        .expect("format verdict gauge");
    }

    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )],
        body,
    )
}

async fn observation(State(state): State<ApiState>) -> Json<ObservationReport> {
    Json(state.snapshot.read().await.report.clone())
}
