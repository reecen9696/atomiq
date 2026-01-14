//! Blockchain-side game transaction processor
//!
//! This module handles game bet transactions ON THE BLOCKCHAIN,
//! ensuring provable fairness by generating VRF outcomes during
//! transaction processing, preventing cherry-picking attacks.

use crate::{
    common::types::{Transaction, TransactionType},
    errors::{AtomiqError, AtomiqResult, StorageError, TransactionError},
    games::{
        types::{CoinChoice, CoinFlipResult, GameOutcome, GameType, Token, VRFBundle},
        vrf_engine::VRFGameEngine,
    },
    game_store,
    storage::OptimizedStorage,
};
use schnorrkel::Keypair;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use hotstuff_rs::block_tree::pluggables::KVGet;

/// Game bet transaction data submitted by players
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameBetData {
    pub game_type: GameType,
    pub bet_amount: u64,
    pub token: Token,
    pub player_choice: CoinChoice,
    pub player_address: String,
}

/// Game result stored in blockchain state
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockchainGameResult {
    pub transaction_id: u64,
    pub player_address: String,
    pub game_type: GameType,
    pub bet_amount: u64,
    pub token: Token,
    pub player_choice: CoinChoice,
    pub coin_result: CoinFlipResult,
    pub outcome: GameOutcome,
    pub vrf_proof: Vec<u8>,
    pub vrf_output: Vec<u8>,
    /// Exact input message that was signed to produce the VRF proof
    pub vrf_input_message: String,
    pub payout: u64,
    pub timestamp: u64,
    pub block_height: u64,
    /// Finalized block hash used as VRF context
    pub block_hash: [u8; 32],
}

/// Blockchain-side game processor that generates VRF outcomes
/// during transaction processing.
pub struct BlockchainGameProcessor {
    /// VRF engine with blockchain's secret key
    vrf_engine: Arc<VRFGameEngine>,
    /// Persistent storage for game results
    storage: Arc<OptimizedStorage>,
    /// Blockchain's keypair for VRF generation
    blockchain_keypair: Keypair,
}

impl BlockchainGameProcessor {
    /// Create a new blockchain game processor with blockchain's keypair.
    pub fn new(storage: Arc<OptimizedStorage>, blockchain_keypair: Keypair) -> Self {
        let vrf_engine = Arc::new(VRFGameEngine::new(blockchain_keypair.clone()));

        Self {
            vrf_engine,
            storage,
            blockchain_keypair,
        }
    }

    /// Create a processor using a persistent VRF key stored in RocksDB.
    ///
    /// This keeps `vrf.public_key` stable across restarts.
    pub fn new_with_persistent_key(storage: Arc<OptimizedStorage>) -> AtomiqResult<Self> {
        let keypair = load_or_create_vrf_keypair(storage.as_ref())?;
        Ok(Self::new(storage, keypair))
    }

    /// Process a game bet transaction AFTER block finalization.
    /// This generates the VRF proof using the finalized block hash, preventing manipulation.
    pub fn process_game_transaction(
        &self,
        transaction: &Transaction,
        block_hash: [u8; 32],
        block_height: u64,
    ) -> AtomiqResult<BlockchainGameResult> {
        // Check if we've already processed this transaction (DB is source of truth)
        if let Some(existing_result) = game_store::load_game_result(self.storage.as_ref(), transaction.id)? {
            if existing_result.block_height == block_height {
                return Ok(existing_result);
            }
        }

        // Verify this is a game bet transaction
        if transaction.tx_type != TransactionType::GameBet {
            return Err(AtomiqError::Transaction(TransactionError::InvalidFormat(
                "Transaction is not a game bet".to_string(),
            )));
        }

        // Deserialize game bet data from transaction payload
        let bet_data: GameBetData = serde_json::from_slice(&transaction.data).map_err(|e| {
            AtomiqError::Transaction(TransactionError::InvalidFormat(format!(
                "Failed to deserialize game bet: {}",
                e
            )))
        })?;

        // Create deterministic context from FINALIZED block data.
        let context = format!(
            "block_hash:{},tx:{},height:{},time:{}",
            hex::encode(block_hash),
            transaction.id,
            block_height,
            transaction.timestamp
        );

        // Generate VRF proof on the blockchain using the deterministic context
        let vrf_bundle = self
            .vrf_engine
            .generate_outcome(
                &format!("tx-{}", transaction.id),
                bet_data.game_type,
                &bet_data.player_address,
                &context,
            )
            .map_err(|e| {
                AtomiqError::Transaction(TransactionError::ExecutionFailed(format!(
                    "VRF generation failed: {}",
                    e
                )))
            })?;

        // Parse VRF output for game logic
        let vrf_output = hex::decode(&vrf_bundle.vrf_output).map_err(|e| {
            AtomiqError::Transaction(TransactionError::ExecutionFailed(format!(
                "VRF decode failed: {}",
                e
            )))
        })?;

        // Determine game outcome from VRF output (deterministic)
        let coin_result = determine_coin_result(&vrf_output);
        let outcome = if (coin_result == CoinFlipResult::Heads && bet_data.player_choice == CoinChoice::Heads)
            || (coin_result == CoinFlipResult::Tails && bet_data.player_choice == CoinChoice::Tails)
        {
            GameOutcome::Win
        } else {
            GameOutcome::Loss
        };

        // Calculate payout based on outcome
        let payout = if outcome == GameOutcome::Win {
            bet_data.bet_amount * 2
        } else {
            0
        };

        let vrf_proof = hex::decode(&vrf_bundle.vrf_proof).map_err(|e| {
            AtomiqError::Transaction(TransactionError::ExecutionFailed(format!(
                "VRF proof decode failed: {}",
                e
            )))
        })?;

        let game_result = BlockchainGameResult {
            transaction_id: transaction.id,
            player_address: bet_data.player_address,
            game_type: bet_data.game_type,
            bet_amount: bet_data.bet_amount,
            token: bet_data.token,
            player_choice: bet_data.player_choice,
            coin_result,
            outcome,
            vrf_proof,
            vrf_output,
            vrf_input_message: vrf_bundle.input_message,
            payout,
            timestamp: transaction.timestamp,
            block_height,
            block_hash,
        };

        game_store::store_game_result(self.storage.as_ref(), &game_result)?;
        Ok(game_result)
    }

    /// Get a game result by transaction ID
    pub fn get_game_result(&self, transaction_id: u64) -> Option<BlockchainGameResult> {
        game_store::load_game_result(self.storage.as_ref(), transaction_id)
            .ok()
            .flatten()
    }

    /// Verify a game result's VRF proof and game logic.
    pub fn verify_game_result(&self, game_result: &BlockchainGameResult) -> AtomiqResult<bool> {
        let context = format!(
            "block_hash:{},tx:{},height:{},time:{}",
            hex::encode(game_result.block_hash),
            game_result.transaction_id,
            game_result.block_height,
            game_result.timestamp
        );

        let expected_input_message = format!(
            "tx-{}:{}:{}:{}",
            game_result.transaction_id,
            game_result.game_type,
            game_result.player_address,
            context
        );

        if game_result.vrf_input_message != expected_input_message {
            return Ok(false);
        }

        let vrf_bundle = VRFBundle {
            vrf_output: hex::encode(&game_result.vrf_output),
            vrf_proof: hex::encode(&game_result.vrf_proof),
            public_key: hex::encode(self.get_public_key()),
            input_message: game_result.vrf_input_message.clone(),
        };

        let vrf_valid = VRFGameEngine::verify_vrf_proof(&vrf_bundle, &expected_input_message)
            .map_err(|e| {
                AtomiqError::Transaction(TransactionError::ExecutionFailed(format!(
                    "VRF verification failed: {}",
                    e
                )))
            })?;
        if !vrf_valid {
            return Ok(false);
        }

        let expected_coin = determine_coin_result(&game_result.vrf_output);
        if expected_coin != game_result.coin_result {
            return Ok(false);
        }

        let expected_outcome = if (expected_coin == CoinFlipResult::Heads && game_result.player_choice == CoinChoice::Heads)
            || (expected_coin == CoinFlipResult::Tails && game_result.player_choice == CoinChoice::Tails)
        {
            GameOutcome::Win
        } else {
            GameOutcome::Loss
        };
        if expected_outcome != game_result.outcome {
            return Ok(false);
        }

        let expected_payout = if expected_outcome == GameOutcome::Win {
            game_result.bet_amount * 2
        } else {
            0
        };
        if expected_payout != game_result.payout {
            return Ok(false);
        }

        Ok(true)
    }

    /// Get all game results (no DB scan helper yet).
    pub fn get_all_game_results(&self) -> Vec<BlockchainGameResult> {
        Vec::new()
    }

    /// Get the blockchain's public key for VRF verification
    pub fn get_public_key(&self) -> Vec<u8> {
        self.blockchain_keypair.public.to_bytes().to_vec()
    }
}

/// Load the VRF public key from RocksDB, if a seed has already been created.
///
/// This is useful for query endpoints that want to return a complete fairness record
/// without depending on an in-memory game processor.
pub fn load_vrf_public_key(storage: &OptimizedStorage) -> AtomiqResult<Option<Vec<u8>>> {
    const VRF_SEED_KEY: &[u8] = b"vrf:mini_secret_seed";

    let Some(existing) = storage.get(VRF_SEED_KEY) else {
        return Ok(None);
    };

    let seed: [u8; 32] = existing.try_into().map_err(|_| {
        AtomiqError::Storage(StorageError::CorruptedData(
            "VRF seed must be 32 bytes".to_string(),
        ))
    })?;

    use schnorrkel::{ExpansionMode, MiniSecretKey};
    let mini = MiniSecretKey::from_bytes(&seed).map_err(|e| {
        AtomiqError::Storage(StorageError::CorruptedData(format!(
            "Invalid VRF seed: {:?}",
            e
        )))
    })?;
    let keypair = mini.expand_to_keypair(ExpansionMode::Ed25519);
    Ok(Some(keypair.public.to_bytes().to_vec()))
}

fn load_or_create_vrf_keypair(storage: &OptimizedStorage) -> AtomiqResult<Keypair> {
    const VRF_SEED_KEY: &[u8] = b"vrf:mini_secret_seed";

    if let Some(existing) = storage.get(VRF_SEED_KEY) {
        let seed: [u8; 32] = existing.try_into().map_err(|_| {
            AtomiqError::Storage(StorageError::CorruptedData(
                "VRF seed must be 32 bytes".to_string(),
            ))
        })?;

        use schnorrkel::{ExpansionMode, MiniSecretKey};
        let mini = MiniSecretKey::from_bytes(&seed).map_err(|e| {
            AtomiqError::Storage(StorageError::CorruptedData(format!(
                "Invalid VRF seed: {:?}",
                e
            )))
        })?;
        return Ok(mini.expand_to_keypair(ExpansionMode::Ed25519));
    }

    use rand_core::OsRng;
    use schnorrkel::{ExpansionMode, MiniSecretKey};

    let mini = MiniSecretKey::generate_with(OsRng);
    let seed_bytes = mini.to_bytes();
    storage
        .put(VRF_SEED_KEY, &seed_bytes)
        .map_err(|e| AtomiqError::Storage(StorageError::WriteFailed(e.to_string())))?;

    Ok(mini.expand_to_keypair(ExpansionMode::Ed25519))
}

/// Determine coin flip result from VRF output (deterministic)
fn determine_coin_result(vrf_output: &[u8]) -> CoinFlipResult {
    let result_byte = vrf_output.first().copied().unwrap_or(0);
    if result_byte % 2 == 0 {
        CoinFlipResult::Heads
    } else {
        CoinFlipResult::Tails
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_processor_creates_deterministic_outcomes() {
        use schnorrkel::MiniSecretKey;

        let secret_key = MiniSecretKey::from_bytes(&[1u8; 32]).unwrap();
        let keypair = secret_key.expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);

        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(OptimizedStorage::new(dir.path()).unwrap());
        let processor = BlockchainGameProcessor::new(storage, keypair);

        let bet_data = GameBetData {
            game_type: GameType::CoinFlip,
            bet_amount: 1000,
            token: Token::sol(),
            player_choice: CoinChoice::Heads,
            player_address: "test_player".to_string(),
        };

        let transaction = Transaction::new_game_bet_with_timestamp(
            1,
            [0u8; 32],
            serde_json::to_vec(&bet_data).unwrap(),
            1,
            1234567890000,
        );

        let block_hash = [42u8; 32];

        let result1 = processor.process_game_transaction(&transaction, block_hash, 100).unwrap();

        let result2 = processor.process_game_transaction(&transaction, block_hash, 100).unwrap();

        assert_eq!(result1.vrf_output, result2.vrf_output);
        assert_eq!(result1.vrf_proof, result2.vrf_proof);
        assert_eq!(result1.coin_result, result2.coin_result);
        assert_eq!(result1.outcome, result2.outcome);
        assert_eq!(result1.payout, result2.payout);
    }

    #[test]
    fn test_vrf_verification() {
        use schnorrkel::MiniSecretKey;

        let secret_key = MiniSecretKey::from_bytes(&[2u8; 32]).unwrap();
        let keypair = secret_key.expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);

        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(OptimizedStorage::new(dir.path()).unwrap());
        let processor = BlockchainGameProcessor::new(storage, keypair);

        let bet_data = GameBetData {
            game_type: GameType::CoinFlip,
            bet_amount: 1000,
            token: Token::sol(),
            player_choice: CoinChoice::Tails,
            player_address: "test_player".to_string(),
        };

        let transaction = Transaction::new_game_bet_with_timestamp(
            1,
            [0u8; 32],
            serde_json::to_vec(&bet_data).unwrap(),
            1,
            1234567890000,
        );

        let block_hash = [123u8; 32];
        let result = processor.process_game_transaction(&transaction, block_hash, 100).unwrap();
        assert!(processor.verify_game_result(&result).unwrap());
    }
}
