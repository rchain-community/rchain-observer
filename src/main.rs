use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use anyhow::{anyhow, Context};
use axum::{extract::State, routing::get, Json, Router};
use clap::{Parser, Subcommand};
use rchain_observer::{model::NodeTarget, observer::Observer};
use serde_json::{json, Value};
use tokio::net::TcpListener;

#[derive(Debug, Parser)]
#[command(name = "rchain-observer")]
#[command(about = "Cross-check finalized state across RChain nodes")]
struct Cli {
    #[arg(long = "node", value_parser = parse_node, required = true)]
    nodes: Vec<NodeTarget>,

    #[arg(long, default_value_t = 3000)]
    timeout_ms: u64,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Once,
    Serve {
        #[arg(long, default_value = "127.0.0.1:8088")]
        listen: String,
    },
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
    let observer = Observer::new(cli.nodes, Duration::from_millis(cli.timeout_ms))?;

    match cli.command {
        Command::Once => {
            let report = observer.observe().await;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Serve { listen } => serve(observer, &listen).await?,
    }

    Ok(())
}

async fn serve(observer: Observer, listen: &str) -> anyhow::Result<()> {
    let address = SocketAddr::from_str(listen)
        .with_context(|| format!("invalid listen address: {listen}"))?;
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/observation", get(observation))
        .with_state(Arc::new(observer));
    let listener = TcpListener::bind(address).await?;
    axum::serve(listener, app)
        .await
        .map_err(|error| anyhow!(error))?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn observation(State(observer): State<Arc<Observer>>) -> Json<Value> {
    Json(serde_json::to_value(observer.observe().await).expect("serializable observation report"))
}
