use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeTarget {
    pub name: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Observation {
    pub node: String,
    #[serde(skip_serializing)]
    pub base_url: String,
    pub reachable: bool,
    pub network_id: Option<String>,
    pub shard_id: Option<String>,
    pub latest_block: Option<u64>,
    pub finalized_block: Option<u64>,
    pub finalized_hash: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerdictClass {
    Agreement,
    HeightSkew,
    Majority,
    Divergence,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Verdict {
    pub class: VerdictClass,
    pub observed_nodes: usize,
    pub comparable_nodes: usize,
    pub agreeing_nodes: usize,
    pub summary: String,
    pub reference: Option<FinalizedIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FinalizedIdentity {
    pub network_id: String,
    pub shard_id: String,
    pub block_number: u64,
    pub block_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeightSummary {
    pub reachable_nodes: usize,
    pub latest_min: Option<u64>,
    pub latest_max: Option<u64>,
    pub finalized_min: Option<u64>,
    pub finalized_max: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservationReport {
    pub observed_at_ms: u64,
    pub heights: HeightSummary,
    pub observations: Vec<Observation>,
    pub verdict: Verdict,
}
