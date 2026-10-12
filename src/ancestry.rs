use std::collections::{HashSet, VecDeque};

use reqwest::Client;

use crate::{
    model::{AncestryCheck, AncestryClass, FinalizedIdentity, NodeTarget},
    raw_block::fetch_verified_block,
};

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

        let block = match fetch_verified_block(client, witness, &entry.hash).await {
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

        if block.shard_id != higher.shard_id {
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
                cryptographically_verified_blocks: examined_blocks,
                path: entry.path,
                reason: "lower finalized block is reachable through a justification DAG decoded only from BLAKE2b-256-verified raw block preimages".to_string(),
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
        cryptographically_verified_blocks: examined_blocks,
        path: Vec::new(),
        reason: "complete cryptographically verified justification traversal reached the target height without the lower finalized hash".to_string(),
    }
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
        cryptographically_verified_blocks: examined_blocks,
        path: Vec::new(),
        reason: reason.to_string(),
    }
}
