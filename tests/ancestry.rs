use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use rchain_observer::{
    model::{AncestryClass, NodeTarget, VerdictClass},
    observer::Observer,
};
use serde_json::{json, Value};
use tokio::net::TcpListener;

#[derive(Clone)]
struct MockNode {
    finalized_height: u64,
    finalized_hash: String,
    blocks: Arc<HashMap<String, Value>>,
    fail_hash: Option<String>,
}

async fn status(State(state): State<MockNode>) -> Json<Value> {
    Json(json!({
        "networkId": "testnet",
        "shardId": "/root",
        "latestBlockNumber": state.finalized_height + 2
    }))
}

async fn finalized(State(state): State<MockNode>) -> Json<Value> {
    Json(json!({
        "blockInfo": {
            "blockNumber": state.finalized_height,
            "blockHash": state.finalized_hash,
            "shardId": "/root"
        }
    }))
}

async fn block(Path(hash): Path<String>, State(state): State<MockNode>) -> Response {
    if state.fail_hash.as_deref() == Some(hash.as_str()) {
        return (StatusCode::SERVICE_UNAVAILABLE, "evidence unavailable").into_response();
    }
    match state.blocks.get(&hash) {
        Some(value) => Json(value.clone()).into_response(),
        None => (StatusCode::NOT_FOUND, "unknown block").into_response(),
    }
}

async fn spawn_node(
    finalized_height: u64,
    finalized_hash: &str,
    blocks: HashMap<String, Value>,
    fail_hash: Option<&str>,
) -> NodeTarget {
    let state = MockNode {
        finalized_height,
        finalized_hash: finalized_hash.to_string(),
        blocks: Arc::new(blocks),
        fail_hash: fail_hash.map(str::to_string),
    };
    let app = Router::new()
        .route("/api/status", get(status))
        .route("/api/last-finalized-block", get(finalized))
        .route("/api/block/{hash}", get(block))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    NodeTarget {
        name: format!("node-{finalized_height}-{finalized_hash}"),
        base_url: format!("http://{address}"),
    }
}

fn evidence(hash: &str, height: u64, justifications: &[&str]) -> Value {
    json!({
        "blockInfo": {
            "blockHash": hash,
            "blockNumber": height,
            "justifications": justifications,
            "shardId": "/root"
        },
        "deploys": []
    })
}

async fn observe_pair(
    higher_blocks: HashMap<String, Value>,
    fail_hash: Option<&str>,
) -> rchain_observer::model::ObservationReport {
    let lower = spawn_node(99, "lower99", HashMap::new(), None).await;
    let higher = spawn_node(100, "higher100", higher_blocks, fail_hash).await;
    Observer::new(vec![lower, higher], Duration::from_secs(1))
        .unwrap()
        .with_ancestry_max_blocks(32)
        .observe()
        .await
}

#[tokio::test]
async fn lower_finalized_hash_in_justification_dag_is_proven_lag() {
    let blocks = HashMap::from([
        (
            "higher100".to_string(),
            evidence("higher100", 100, &["lower99"]),
        ),
        ("lower99".to_string(), evidence("lower99", 99, &[])),
    ]);
    let report = observe_pair(blocks, None).await;

    assert_eq!(report.verdict.class, VerdictClass::ProvenLag);
    assert_eq!(report.ancestry_checks.len(), 1);
    let check = &report.ancestry_checks[0];
    assert_eq!(check.class, AncestryClass::ProvenLag);
    assert_eq!(check.path, vec!["higher100", "lower99"]);
    assert_eq!(check.examined_blocks, 2);
}

#[tokio::test]
async fn complete_dag_without_lower_hash_is_conflicting_finalized_history() {
    let blocks = HashMap::from([
        (
            "higher100".to_string(),
            evidence("higher100", 100, &["other99"]),
        ),
        ("other99".to_string(), evidence("other99", 99, &[])),
    ]);
    let report = observe_pair(blocks, None).await;

    assert_eq!(
        report.verdict.class,
        VerdictClass::ConflictingFinalizedHistory
    );
    assert_eq!(report.ancestry_checks.len(), 1);
    assert_eq!(
        report.ancestry_checks[0].class,
        AncestryClass::ConflictingFinalizedHistory
    );
}

#[tokio::test]
async fn missing_block_evidence_keeps_height_skew_unresolved() {
    let blocks = HashMap::from([(
        "higher100".to_string(),
        evidence("higher100", 100, &["missing99"]),
    )]);
    let report = observe_pair(blocks, Some("missing99")).await;

    assert_eq!(report.verdict.class, VerdictClass::HeightSkew);
    assert_eq!(report.ancestry_checks.len(), 1);
    assert_eq!(
        report.ancestry_checks[0].class,
        AncestryClass::UnresolvedHeightSkew
    );
    assert!(report.ancestry_checks[0]
        .reason
        .contains("503 Service Unavailable"));
}
