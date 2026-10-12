use std::collections::{HashMap, HashSet};

use crate::model::{
    AncestryCheck, AncestryClass, FinalizedIdentity, Observation, Verdict, VerdictClass,
};

pub fn classify(observations: &[Observation]) -> Verdict {
    let identities = observations
        .iter()
        .filter_map(finalized_identity)
        .collect::<Vec<_>>();

    let comparable_nodes = identities.len();
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

    let mut exact_groups: HashMap<FinalizedIdentity, usize> = HashMap::new();
    for identity in &identities {
        *exact_groups.entry(identity.clone()).or_insert(0) += 1;
    }

    let (reference, agreeing_nodes) = select_reference(&exact_groups);
    if exact_groups.len() == 1 {
        return Verdict {
            class: VerdictClass::Agreement,
            observed_nodes: observations.len(),
            comparable_nodes,
            agreeing_nodes,
            summary: format!("{agreeing_nodes}/{comparable_nodes} comparable nodes agree"),
            reference: Some(reference),
        };
    }

    let network_shards = identities
        .iter()
        .map(|value| (&value.network_id, &value.shard_id))
        .collect::<HashSet<_>>();
    if network_shards.len() > 1 {
        return Verdict {
            class: VerdictClass::Divergence,
            observed_nodes: observations.len(),
            comparable_nodes,
            agreeing_nodes,
            summary: "comparable nodes report different network or shard identities".to_string(),
            reference: Some(reference),
        };
    }

    let mut hashes_by_height: HashMap<u64, HashSet<&str>> = HashMap::new();
    for identity in &identities {
        hashes_by_height
            .entry(identity.block_number)
            .or_default()
            .insert(identity.block_hash.as_str());
    }
    let same_height_hash_conflict = hashes_by_height.values().any(|hashes| hashes.len() > 1);

    if !same_height_hash_conflict {
        let min_height = identities
            .iter()
            .map(|value| value.block_number)
            .min()
            .expect("non-empty finalized identities");
        let max_height = identities
            .iter()
            .map(|value| value.block_number)
            .max()
            .expect("non-empty finalized identities");
        return Verdict {
            class: VerdictClass::HeightSkew,
            observed_nodes: observations.len(),
            comparable_nodes,
            agreeing_nodes,
            summary: format!(
                "finalized height skew {min_height}..{max_height}; no same-height hash conflict observed; ancestry not established"
            ),
            reference: Some(reference),
        };
    }

    let (class, summary) = if agreeing_nodes * 2 > comparable_nodes {
        (
            VerdictClass::Majority,
            format!(
                "{agreeing_nodes}/{comparable_nodes} comparable nodes share one finalized identity; conflicting finalized hash observed"
            ),
        )
    } else {
        (
            VerdictClass::Divergence,
            format!(
                "no strict majority across {comparable_nodes} comparable nodes; conflicting finalized hash observed"
            ),
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

fn finalized_identity(observation: &Observation) -> Option<FinalizedIdentity> {
    Some(FinalizedIdentity {
        network_id: observation.network_id.clone()?,
        shard_id: observation.shard_id.clone()?,
        block_number: observation.finalized_block?,
        block_hash: observation.finalized_hash.clone()?,
    })
}

fn select_reference(groups: &HashMap<FinalizedIdentity, usize>) -> (FinalizedIdentity, usize) {
    groups
        .iter()
        .max_by(|(left_id, left_count), (right_id, right_count)| {
            left_id
                .block_number
                .cmp(&right_id.block_number)
                .then_with(|| left_count.cmp(right_count))
                .then_with(|| left_id.block_hash.cmp(&right_id.block_hash))
        })
        .map(|(identity, count)| (identity.clone(), *count))
        .expect("non-empty finalized groups")
}

pub fn refine_with_ancestry(mut verdict: Verdict, checks: &[AncestryCheck]) -> Verdict {
    if verdict.class != VerdictClass::HeightSkew || checks.is_empty() {
        return verdict;
    }

    let conflicting = checks
        .iter()
        .filter(|check| check.class == AncestryClass::ConflictingFinalizedHistory)
        .count();
    let proven = checks
        .iter()
        .filter(|check| check.class == AncestryClass::ProvenLag)
        .count();
    let unresolved = checks.len().saturating_sub(conflicting + proven);

    if conflicting > 0 {
        verdict.class = VerdictClass::ConflictingFinalizedHistory;
        verdict.summary = format!(
            "{conflicting}/{} ancestry checks prove finalized histories that do not connect to the highest finalized reference",
            checks.len()
        );
    } else if unresolved == 0 && proven == checks.len() {
        verdict.class = VerdictClass::ProvenLag;
        verdict.summary = format!(
            "{proven}/{} lower finalized identities are proven ancestors of the highest finalized reference",
            checks.len()
        );
    } else {
        verdict.class = VerdictClass::HeightSkew;
        verdict.summary = format!(
            "finalized height skew remains unresolved: {proven} proven ancestry checks, {unresolved} unresolved"
        );
    }

    verdict
}
