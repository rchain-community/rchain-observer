use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use anyhow::{anyhow, Context};
use axum::{extract::State, routing::get, Json, Router};
use clap::{Parser, Subcommand};
use rchain_observer::{model::NodeTarget, observer::Observer};
use serde_json::{json, Value};
use tokio::{
    net::TcpListener,
    sync::RwLock,
    time::{self, MissedTickBehavior},
};

#[derive(Debug, Parser)]
#[command(name = "rchain-observer")]
#[command(about = "Cross-check finalized state across RChain nodes")]
struct Cli {
    #[arg(long = "node", value_parser = parse_node, required = true)]
    nodes: Vec<NodeTarget>,

    #[arg(long, default_value_t = 3000)]
    timeout_ms: u64,

    #[arg(long, default_value_t = 512)]
    ancestry_max_blocks: usize,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Once,
    Serve {
        #[arg(long, default_value = "127.0.0.1:8088")]
        listen: String,
        #[arg(long, default_value_t = 5000)]
        refresh_ms: u64,
    },
}

#[derive(Clone)]
struct ApiState {
    snapshot: Arc<RwLock<rchain_observer::model::ObservationReport>>,
}

fn parse_node(value: &str) -> Result<NodeTarget, String> {
    let (name, base_url) = value
        .split_once('=')
        .ok_or_else(|| "node must be NAME=URL".to_string())?;
    if name.trim().is_empty() || base_url.trim().is_empty() {
        return Err("node must be NAME=URL".to_string());
    }
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return Err("node URL must start with http:// or https://".to_string());
    }
    Ok(NodeTarget {
        name: name.to_string(),
        base_url: base_url.to_string(),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.ancestry_max_blocks == 0 {
        return Err(anyhow!("ancestry-max-blocks must be at least 1"));
    }
    let observer = Observer::new(cli.nodes, Duration::from_millis(cli.timeout_ms))?
        .with_ancestry_max_blocks(cli.ancestry_max_blocks);

    match cli.command {
        Command::Once => {
            let report = observer.observe().await;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Serve { listen, refresh_ms } => serve(observer, &listen, refresh_ms).await?,
    }

    Ok(())
}

async fn serve(observer: Observer, listen: &str, refresh_ms: u64) -> anyhow::Result<()> {
    if refresh_ms < 1000 {
        return Err(anyhow!("refresh-ms must be at least 1000"));
    }

    let address = SocketAddr::from_str(listen)
        .with_context(|| format!("invalid listen address: {listen}"))?;
    let initial = observer.observe().await;
    let state = ApiState {
        snapshot: Arc::new(RwLock::new(initial)),
    };

    let refresh_state = state.clone();
    let refresh_observer = observer.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(refresh_ms));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            let next = refresh_observer.observe().await;
            *refresh_state.snapshot.write().await = next;
        }
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/observation", get(observation))
        .with_state(state);
    let listener = TcpListener::bind(address).await?;
    axum::serve(listener, app)
        .await
        .map_err(|error| anyhow!(error))?;
    Ok(())
}

async fn health(State(state): State<ApiState>) -> Json<Value> {
    let snapshot = state.snapshot.read().await;
    Json(json!({
        "status": "ok",
        "observedAtMs": snapshot.observed_at_ms,
        "observedNodes": snapshot.verdict.observed_nodes,
        "reachableNodes": snapshot.heights.reachable_nodes,
        "verdict": snapshot.verdict.class,
        "ancestryChecks": snapshot.ancestry_checks.len(),
    }))
}

async fn observation(
    State(state): State<ApiState>,
) -> Json<rchain_observer::model::ObservationReport> {
    Json(state.snapshot.read().await.clone())
}
