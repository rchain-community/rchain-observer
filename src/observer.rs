use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::future::join_all;
use reqwest::Client;
use serde::Deserialize;

use crate::{
    model::{HeightSummary, NodeTarget, Observation, ObservationReport},
    verdict::classify,
};

#[derive(Debug, Deserialize)]
struct StatusResponse {
    #[serde(rename = "networkId")]
    network_id: Option<String>,
    #[serde(rename = "shardId")]
    shard_id: Option<String>,
    #[serde(rename = "latestBlockNumber")]
    latest_block_number: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct FinalizedResponse {
    #[serde(rename = "blockInfo")]
    block_info: Option<BlockInfo>,
}

#[derive(Debug, Deserialize)]
struct BlockInfo {
    #[serde(rename = "blockNumber")]
    block_number: Option<u64>,
    #[serde(rename = "blockHash")]
    block_hash: Option<String>,
    #[serde(rename = "shardId")]
    shard_id: Option<String>,
}

#[derive(Clone)]
pub struct Observer {
    client: Client,
    nodes: Vec<NodeTarget>,
}

impl Observer {
    pub fn new(nodes: Vec<NodeTarget>, timeout: Duration) -> anyhow::Result<Self> {
        let client = Client::builder().timeout(timeout).build()?;
        Ok(Self { client, nodes })
    }

    pub async fn observe(&self) -> ObservationReport {
        let observations = join_all(
            self.nodes
                .iter()
                .cloned()
                .map(|node| observe_node(self.client.clone(), node)),
        )
        .await;
        let verdict = classify(&observations);
        let heights = summarize_heights(&observations);
        let observed_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        ObservationReport {
            observed_at_ms,
            heights,
            observations,
            verdict,
        }
    }
}

async fn observe_node(client: Client, node: NodeTarget) -> Observation {
    let base = node.base_url.trim_end_matches('/');
    let status_url = format!("{base}/api/status");
    let finalized_url = format!("{base}/api/last-finalized-block");

    let (status_result, finalized_result) = tokio::join!(
        client.get(status_url).send(),
        client.get(finalized_url).send()
    );

    let status = match status_result {
        Ok(response) => match response.error_for_status() {
            Ok(response) => response.json::<StatusResponse>().await,
            Err(error) => return failed(&node, error.to_string()),
        },
        Err(error) => return failed(&node, error.to_string()),
    };

    let finalized = match finalized_result {
        Ok(response) => match response.error_for_status() {
            Ok(response) => response.json::<FinalizedResponse>().await,
            Err(error) => return failed(&node, error.to_string()),
        },
        Err(error) => return failed(&node, error.to_string()),
    };

    match (status, finalized) {
        (Ok(status), Ok(finalized)) => {
            let block = finalized.block_info;
            let finalized_shard = block.as_ref().and_then(|value| value.shard_id.clone());
            Observation {
                node: node.name,
                base_url: node.base_url,
                reachable: true,
                network_id: status.network_id,
                shard_id: status.shard_id.or(finalized_shard),
                latest_block: status.latest_block_number,
                finalized_block: block.as_ref().and_then(|value| value.block_number),
                finalized_hash: block.and_then(|value| value.block_hash),
                error: None,
            }
        }
        (Err(error), _) | (_, Err(error)) => failed(&node, error.to_string()),
    }
}

fn failed(node: &NodeTarget, error: String) -> Observation {
    let error = error.replace(&node.base_url, "<node>");
    Observation {
        node: node.name.clone(),
        base_url: node.base_url.clone(),
        reachable: false,
        network_id: None,
        shard_id: None,
        latest_block: None,
        finalized_block: None,
        finalized_hash: None,
        error: Some(error),
    }
}

fn summarize_heights(observations: &[Observation]) -> HeightSummary {
    let latest = observations.iter().filter_map(|value| value.latest_block);
    let finalized = observations
        .iter()
        .filter_map(|value| value.finalized_block);
    let latest_min = latest.clone().min();
    let latest_max = latest.max();
    let finalized_min = finalized.clone().min();
    let finalized_max = finalized.max();

    HeightSummary {
        reachable_nodes: observations.iter().filter(|value| value.reachable).count(),
        latest_min,
        latest_max,
        finalized_min,
        finalized_max,
    }
}
