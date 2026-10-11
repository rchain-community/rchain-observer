use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeTarget {
    pub name: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Observation {
    pub node: String,
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
pub struct ObservationReport {
    pub observations: Vec<Observation>,
    pub verdict: Verdict,
}
