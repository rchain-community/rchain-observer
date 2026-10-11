use std::collections::HashMap;

use crate::model::{FinalizedIdentity, Observation, Verdict, VerdictClass};

pub fn classify(observations: &[Observation]) -> Verdict {
    let mut groups: HashMap<FinalizedIdentity, usize> = HashMap::new();

    for observation in observations {
        let identity = match (
            observation.network_id.as_ref(),
            observation.shard_id.as_ref(),
            observation.finalized_block,
            observation.finalized_hash.as_ref(),
        ) {
            (Some(network_id), Some(shard_id), Some(block_number), Some(block_hash)) => {
                FinalizedIdentity {
                    network_id: network_id.clone(),
                    shard_id: shard_id.clone(),
                    block_number,
                    block_hash: block_hash.clone(),
                }
            }
            _ => continue,
        };

        *groups.entry(identity).or_insert(0) += 1;
    }

    let comparable_nodes = groups.values().sum::<usize>();
    if comparable_nodes == 0 {
        return Verdict {
            class: VerdictClass::Unavailable,
            observed_nodes: observations.len(),
            comparable_nodes: 0,
            agreeing_nodes: 0,
            summary: "no node returned a comparable finalized snapshot".to_string(),
            reference: None,
        };
    }

    let (reference, agreeing_nodes) = groups
        .into_iter()
        .max_by(|(left_id, left_count), (right_id, right_count)| {
            left_count
                .cmp(right_count)
                .then_with(|| right_id.block_number.cmp(&left_id.block_number))
                .then_with(|| right_id.block_hash.cmp(&left_id.block_hash))
        })
        .expect("non-empty finalized groups");

    let (class, summary) = if agreeing_nodes == comparable_nodes {
        (
            VerdictClass::Agreement,
            format!("{agreeing_nodes}/{comparable_nodes} comparable nodes agree"),
        )
    } else if agreeing_nodes * 2 > comparable_nodes {
        (
            VerdictClass::Majority,
            format!(
                "{agreeing_nodes}/{comparable_nodes} comparable nodes agree; divergence observed"
            ),
        )
    } else {
        (
            VerdictClass::Divergence,
            format!("no strict majority across {comparable_nodes} comparable nodes"),
        )
    };

    Verdict {
        class,
        observed_nodes: observations.len(),
        comparable_nodes,
        agreeing_nodes,
        summary,
        reference: Some(reference),
    }
}
