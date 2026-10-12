use std::collections::{HashSet, VecDeque};

use reqwest::Client;
use serde::Deserialize;

use crate::model::{AncestryCheck, AncestryClass, FinalizedIdentity, NodeTarget};

#[derive(Debug, Deserialize)]
struct BlockResponse {
    #[serde(rename = "blockInfo")]
    block_info: Option<BlockEvidence>,
}

#[derive(Debug, Deserialize)]
struct BlockEvidence {
    #[serde(rename = "blockHash")]
    block_hash: String,
    #[serde(rename = "blockNumber")]
    block_number: u64,
    #[serde(rename = "shardId")]
    shard_id: Option<String>,
    #[serde(default)]
    justifications: Vec<String>,
}

#[derive(Debug)]
struct QueueEntry {
    hash: String,
    path: Vec<String>,
    parent_height: Option<u64>,
}

pub async fn verify_ancestry(
    client: &Client,
    witness: &NodeTarget,
    lower: &FinalizedIdentity,
    higher: &FinalizedIdentity,
    max_blocks: usize,
) -> AncestryCheck {
    if lower.network_id != higher.network_id || lower.shard_id != higher.shard_id {
        return unresolved(
            witness,
            lower,
            higher,
            0,
            "network or shard identity differs",
        );
    }

    if lower.block_number >= higher.block_number {
        return unresolved(
            witness,
            lower,
            higher,
            0,
            "ancestry verification requires a strictly lower finalized height",
        );
    }

    if max_blocks == 0 {
        return unresolved(witness, lower, higher, 0, "ancestry block budget is zero");
    }

    let mut queue = VecDeque::from([QueueEntry {
        hash: higher.block_hash.clone(),
        path: vec![higher.block_hash.clone()],
        parent_height: None,
    }]);
    let mut seen = HashSet::new();
    let mut examined_blocks = 0usize;

    while let Some(entry) = queue.pop_front() {
        if !seen.insert(entry.hash.clone()) {
            continue;
        }

        if examined_blocks >= max_blocks {
            return unresolved(
                witness,
                lower,
                higher,
                examined_blocks,
                &format!("ancestry traversal exceeded the {max_blocks}-block evidence budget"),
            );
        }

        let block = match fetch_block(client, witness, &entry.hash).await {
            Ok(block) => block,
            Err(reason) => return unresolved(witness, lower, higher, examined_blocks, &reason),
        };
        examined_blocks += 1;

        if block.block_hash != entry.hash {
            return unresolved(
                witness,
                lower,
                higher,
                examined_blocks,
                "node returned a block hash different from the requested content address",
            );
        }

        if block.shard_id.as_deref() != Some(higher.shard_id.as_str()) {
            return unresolved(
                witness,
                lower,
                higher,
                examined_blocks,
                "block evidence changed or omitted the expected shard identity",
            );
        }

        if entry.parent_height.is_none() && block.block_number != higher.block_number {
            return unresolved(
                witness,
                lower,
                higher,
                examined_blocks,
                "highest finalized block was returned with an unexpected height",
            );
        }

        if let Some(parent_height) = entry.parent_height {
            if block.block_number >= parent_height {
                return unresolved(
                    witness,
                    lower,
                    higher,
                    examined_blocks,
                    "justification edge did not descend in block height",
                );
            }
        }

        if block.block_hash == lower.block_hash {
            if block.block_number != lower.block_number {
                return unresolved(
                    witness,
                    lower,
                    higher,
                    examined_blocks,
                    "target block hash was returned with an unexpected height",
                );
            }
            return AncestryCheck {
                class: AncestryClass::ProvenLag,
                lower: lower.clone(),
                higher: higher.clone(),
                witness_node: Some(witness.name.clone()),
                examined_blocks,
                path: entry.path,
                reason: "lower finalized block is reachable from the higher finalized block through the node-reported justification DAG".to_string(),
            };
        }

        if block.block_number < lower.block_number {
            continue;
        }

        if block.block_number == lower.block_number {
            continue;
        }

        for justification in block.justifications {
            let mut path = entry.path.clone();
            path.push(justification.clone());
            queue.push_back(QueueEntry {
                hash: justification,
                path,
                parent_height: Some(block.block_number),
            });
        }
    }

    AncestryCheck {
        class: AncestryClass::ConflictingFinalizedHistory,
        lower: lower.clone(),
        higher: higher.clone(),
        witness_node: Some(witness.name.clone()),
        examined_blocks,
        path: Vec::new(),
        reason: "complete justification traversal reached the target height without the lower finalized hash".to_string(),
    }
}

async fn fetch_block(
    client: &Client,
    witness: &NodeTarget,
    hash: &str,
) -> Result<BlockEvidence, String> {
    let base = witness.base_url.trim_end_matches('/');
    let url = format!("{base}/api/block/{hash}");
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| sanitize_error(witness, error.to_string()))?;
    let response = response
        .error_for_status()
        .map_err(|error| sanitize_error(witness, error.to_string()))?;
    let response = response
        .json::<BlockResponse>()
        .await
        .map_err(|error| sanitize_error(witness, error.to_string()))?;
    response
        .block_info
        .ok_or_else(|| "block response did not include blockInfo".to_string())
}

fn sanitize_error(witness: &NodeTarget, error: String) -> String {
    error.replace(&witness.base_url, "<node>")
}

fn unresolved(
    witness: &NodeTarget,
    lower: &FinalizedIdentity,
    higher: &FinalizedIdentity,
    examined_blocks: usize,
    reason: &str,
) -> AncestryCheck {
    AncestryCheck {
        class: AncestryClass::UnresolvedHeightSkew,
        lower: lower.clone(),
        higher: higher.clone(),
        witness_node: Some(witness.name.clone()),
        examined_blocks,
        path: Vec::new(),
        reason: reason.to_string(),
    }
}
