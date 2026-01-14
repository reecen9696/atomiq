//! Persistent game/fairness records stored in RocksDB.

use crate::{
    blockchain_game_processor::BlockchainGameResult,
    errors::{AtomiqError, AtomiqResult, StorageError},
    storage::OptimizedStorage,
};
use hotstuff_rs::block_tree::pluggables::KVGet;

const GAME_RESULT_PREFIX: &str = "game:result:tx:";

fn game_result_key(tx_id: u64) -> Vec<u8> {
    format!("{}{}", GAME_RESULT_PREFIX, tx_id).into_bytes()
}

pub fn load_game_result(storage: &OptimizedStorage, tx_id: u64) -> AtomiqResult<Option<BlockchainGameResult>> {
    let key = game_result_key(tx_id);
    let Some(bytes) = storage.get(&key) else {
        return Ok(None);
    };

    let result: BlockchainGameResult = serde_json::from_slice(&bytes).map_err(|e| {
        AtomiqError::Storage(StorageError::CorruptedData(format!(
            "Failed to decode game result for tx {}: {}",
            tx_id, e
        )))
    })?;

    Ok(Some(result))
}

pub fn store_game_result(storage: &OptimizedStorage, result: &BlockchainGameResult) -> AtomiqResult<()> {
    let key = game_result_key(result.transaction_id);
    let bytes = serde_json::to_vec(result).map_err(|e| {
        AtomiqError::Storage(StorageError::WriteFailed(format!(
            "Failed to encode game result for tx {}: {}",
            result.transaction_id, e
        )))
    })?;

    storage
        .put(&key, &bytes)
        .map_err(|e| AtomiqError::Storage(StorageError::WriteFailed(e.to_string())))?;

    Ok(())
}
