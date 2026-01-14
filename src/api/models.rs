//! API Response Models
//! 
//! All response types for the API endpoints.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

/// Node status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub node_info: NodeInfo,
    pub sync_info: SyncInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub network: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfo {
    pub latest_block_height: u64,
    pub latest_block_hash: String,
    pub latest_block_time: DateTime<Utc>,
    pub catching_up: bool,
}

/// Block list response (paginated)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlocksResponse {
    pub blocks: Vec<BlockSummary>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSummary {
    pub height: u64,
    pub hash: String,
    pub time: DateTime<Utc>,
    pub tx_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub from: u64,
    pub to: u64,
    pub total_returned: usize,
}

/// Block detail response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDetailResponse {
    pub height: u64,
    pub hash: String,
    pub prev_hash: String,
    pub time: DateTime<Utc>,
    pub tx_count: usize,
    pub tx_ids: Vec<String>,
    pub transactions_root: String,
    pub state_root: String,
}

/// Transaction detail response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub tx_id: String,
    pub included_in: InclusionInfo,
    #[serde(rename = "type")]
    pub tx_type: String,
    pub data: TransactionData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InclusionInfo {
    pub block_height: u64,
    pub block_hash: String,
    pub index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionData {
    pub sender: String,
    pub data: String,
    pub timestamp: u64,
    pub nonce: u64,
}

/// Performance metrics response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsResponse {
    pub performance: PerformanceInfo,
    pub cache: CacheInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceInfo {
    pub total_requests: u64,
    pub avg_response_time_us: u64,
    pub current_concurrent: usize,
    pub max_concurrent: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheInfo {
    pub hit_ratio: f64,
    pub cached_blocks: usize,
    pub cached_transactions: usize,
}
