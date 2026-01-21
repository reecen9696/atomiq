//! Casino Games API - Blockchain-Integrated Edition
//!
//! This module provides HTTP endpoints for provably fair casino games.
//! ALL game outcomes are generated ON-CHAIN by the blockchain's VRF engine,
//! ensuring verifiable fairness and preventing cherry-picking attacks.

use crate::{
    blockchain_game_processor::{BlockchainGameProcessor, GameBetData},
    blockchain_game_processor::SettlementStatus,
    common::types::Transaction,
    api::storage::ApiStorage,
    api::models::{RecentGamesResponse, RecentGameSummary},
    fairness::{FairnessError, FairnessWaiter},
    games::{
        types::{CoinFlipPlayRequest, GameResponse, Token, VerifyVRFRequest, VerifyVRFResponse, GameType},
    },
    storage::OptimizedStorage,
    finalization::FinalizationWaiter,
    game_store,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use std::sync::Arc;
use serde::Deserialize;

/// Recent games query parameters
#[derive(Debug, Deserialize)]
pub struct RecentGamesQuery {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub cursor: Option<String>,
}

/// Recent casino games (newest-first)
/// GET /api/games/recent?limit={n}&cursor={hex}
pub async fn recent_games(
    State(state): State<GameApiState>,
    Query(params): Query<RecentGamesQuery>,
) -> Result<Json<RecentGamesResponse>, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(50).min(200);

    let (tx_ids, next_cursor) = game_store::load_recent_game_tx_ids(
        state.storage.as_ref(),
        params.cursor.as_deref(),
        limit,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load recent games index: {}", e),
        )
    })?;

    let mut games = Vec::with_capacity(tx_ids.len());
    for tx_id in tx_ids {
        let Ok(Some(result)) = game_store::load_game_result(state.storage.as_ref(), tx_id) else {
            continue;
        };

        games.push(RecentGameSummary {
            game_id: format!("tx-{}", result.transaction_id),
            tx_id: result.transaction_id,
            processed: result.settlement_status != SettlementStatus::PendingSettlement,
            settlement_status: result.settlement_status.clone(),
            solana_tx_id: result.solana_tx_id.clone(),
            settlement_error: result.settlement_error.clone(),
            settlement_completed_at: result.settlement_completed_at,
            player_id: result.player_address,
            game_type: result.game_type,
            token: result.token,
            bet_amount: result.bet_amount,
            player_choice: result.player_choice,
            coin_result: result.coin_result,
            outcome: result.outcome,
            payout: result.payout,
            timestamp: result.timestamp,
            block_height: result.block_height,
            block_hash: hex::encode(result.block_hash),
        });
    }

    Ok(Json(RecentGamesResponse { games, next_cursor }))
}

/// Shared state for game API
#[derive(Clone)]
pub struct GameApiState {
    /// Blockchain storage for querying game results
    pub storage: Arc<OptimizedStorage>,
    
    /// Blockchain game processor (for verification and querying)
    pub game_processor: Arc<BlockchainGameProcessor>,
    
    /// Transaction submission channel (to blockchain)
    pub tx_sender: tokio::sync::mpsc::Sender<Transaction>,
    
    /// Finalization waiter for awaiting block commits (optional - without this, games return pending status)
    pub finalization_waiter: Option<Arc<FinalizationWaiter>>,

    /// Fairness waiter for awaiting persisted game results (optional - without this, games may race and return pending)
    pub fairness_waiter: Option<Arc<FairnessWaiter>>,
}

/// Play coin flip game by processing transaction immediately on blockchain
/// POST /api/coinflip/play
pub async fn play_coinflip(
    State(state): State<GameApiState>,
    Json(request): Json<CoinFlipPlayRequest>,
) -> Result<Json<GameResponse>, (StatusCode, String)> {
    // Create game bet data (convert bet amount to u64 lamports/smallest unit)
    let bet_amount_u64 = (request.bet_amount * 1_000_000_000.0) as u64; // Convert SOL to lamports
    let bet_data = GameBetData {
        game_type: GameType::CoinFlip,
        bet_amount: bet_amount_u64,
        token: request.token.clone(),
        player_choice: request.choice,
        player_address: request.player_id.clone(),
    };
    
    // Serialize bet data
    let bet_data_bytes = serde_json::to_vec(&bet_data)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Serialization error: {}", e)))?;
    
    // Generate transaction ID (in production, use proper ID generation)
    let tx_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    
    // Create game bet transaction
    let transaction = Transaction::new_game_bet(
        tx_id,
        [0u8; 32], // In production, use player's actual public key hash
        bet_data_bytes,
        1, // Nonce - in production, track per-player nonces
    );
    
    // IMPORTANT: Register finalization waiter BEFORE submitting transaction
    // This prevents race condition where block is created before waiter is registered
    let finalization_future = if let Some(finalization_waiter) = &state.finalization_waiter {
        let timeout = std::time::Duration::from_secs(10);
        tracing::debug!("Pre-registering finalization waiter for transaction {}", tx_id);
        Some(finalization_waiter.wait_for_transaction(tx_id, timeout))
    } else {
        None
    };
    
    // Submit transaction to blockchain AFTER waiter is registered
    tracing::debug!("Submitting transaction {} to blockchain", tx_id);
    match state.tx_sender.try_send(transaction.clone()) {
        Ok(()) => {}
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "Transaction queue is full; retry shortly".to_string(),
            ));
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Transaction submission channel is closed".to_string(),
            ));
        }
    }
    tracing::debug!("Transaction {} submitted successfully", tx_id);
    
    // Now wait for the finalization result
    if let Some(finalization_future) = finalization_future {
        tracing::info!("Waiting for finalization of transaction {} (timeout: 10s)", tx_id);
        let wait_start = std::time::Instant::now();
        let finalization_result = finalization_future.await;
        let wait_duration = wait_start.elapsed();
        tracing::debug!("Finalization wait completed in {:?}", wait_duration);
    
    match finalization_result {
        Ok(block_event) => {
            tracing::info!("Transaction {} finalized in block {} (height: {})", tx_id, hex::encode(&block_event.hash[..8]), block_event.height);
            // Verify transaction is in the committed block
            if !block_event.contains_transaction(tx_id) {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Transaction {} not found in committed block {}", tx_id, block_event.height)
                ));
            }
            
            // Get actual block hash and height from committed block
            let block_hash = block_event.hash;
            let block_height = block_event.height;
            
            // Wait for fairness record to be persisted by the background fairness worker.
            // This keeps request path light and avoids adding work to the block commit hot path.
            let result = if let Some(fairness_waiter) = &state.fairness_waiter {
                let timeout = std::time::Duration::from_secs(10);
                match fairness_waiter
                    .wait_for_game_result(tx_id, block_height, block_hash, timeout)
                    .await
                {
                    Ok(result) => result,
                    Err(FairnessError::Timeout { .. }) => {
                        return Ok(Json(GameResponse::Pending {
                            game_id: format!("tx-{}", tx_id),
                            message: Some(
                                "Transaction finalized but fairness record is not yet available. Retry shortly."
                                    .to_string(),
                            ),
                        }));
                    }
                    Err(e) => {
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Fairness wait failed: {}", e),
                        ));
                    }
                }
            } else {
                // Fallback: compute and persist inline (legacy behavior)
                state
                    .game_processor
                    .process_game_transaction(&transaction, block_hash, block_height)
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Game processing failed: {}", e)))?
            };
            
            // Convert to API response format
            use crate::games::types::{PlayerInfo, PaymentInfo, VRFBundle, GameData, CoinChoice};
            
            let game_result = crate::games::types::GameResult {
                game_id: format!("tx-{}", tx_id),
                game_type: result.game_type,
                player: PlayerInfo {
                    player_id: result.player_address,
                    wallet_signature: None,
                },
                payment: PaymentInfo {
                    token: result.token,
                    bet_amount: result.bet_amount as f64 / 1_000_000_000.0, // Convert lamports to SOL
                    payout_amount: result.payout as f64 / 1_000_000_000.0,
                    settlement_tx_id: None,
                },
                vrf: VRFBundle {
                    vrf_output: hex::encode(&result.vrf_output),
                    vrf_proof: hex::encode(&result.vrf_proof),
                    public_key: hex::encode(state.game_processor.get_public_key()),
                    input_message: result.vrf_input_message.clone(),
                },
                outcome: result.outcome,
                timestamp: result.timestamp,
                game_data: GameData::CoinFlip {
                    player_choice: result.player_choice,
                    result_choice: match result.coin_result {
                        crate::games::types::CoinFlipResult::Heads => CoinChoice::Heads,
                        crate::games::types::CoinFlipResult::Tails => CoinChoice::Tails,
                    },
                },
                metadata: Some(serde_json::json!({
                    "block_height": block_height,
                    "block_hash": hex::encode(block_hash),
                    "finalization_confirmed": true
                })),
            };
            
            // Return complete game result with finalization proof
            Ok(Json(GameResponse::Complete {
                game_id: format!("tx-{}", tx_id),
                result: game_result,
            }))
        }
        Err(e) => {
            // Timeout or error - return pending status
            log::warn!("Game finalization timeout for tx {}: {}", tx_id, e);
            Ok(Json(GameResponse::Pending {
                game_id: format!("tx-{}", tx_id),
                message: Some(format!(
                    "Transaction submitted but not yet finalized. Please check status in a moment. ({})",
                    e
                )),
            }))
        }
    }
    } else {
        // No finalization waiter available - return pending response immediately
        Ok(Json(GameResponse::Pending {
            game_id: format!("tx-{}", tx_id),
            message: Some("Transaction submitted. Check status with GET /api/game/:id".to_string()),
        }))
    }
}

/// Get game result by transaction ID
/// GET /api/game/:id
pub async fn get_game_result(
    Path(game_id): Path<String>,
    State(state): State<GameApiState>,
) -> Result<Json<GameResponse>, (StatusCode, String)> {
    // Extract transaction ID from game_id (format: "tx-{id}")
    let tx_id = game_id.strip_prefix("tx-")
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Invalid game ID format".to_string()))?;
    
    // Query game result from blockchain
    if let Some(result) = state.game_processor.get_game_result(tx_id) {
        // Convert BlockchainGameResult to GameResult format expected by API
        use crate::games::types::{PlayerInfo, PaymentInfo, VRFBundle, GameData, CoinChoice};
        
        let game_result = crate::games::types::GameResult {
            game_id: game_id.clone(),
            game_type: result.game_type,
            player: PlayerInfo {
                player_id: result.player_address,
                wallet_signature: None,
            },
            payment: PaymentInfo {
                token: result.token,
                bet_amount: result.bet_amount as f64 / 1_000_000_000.0, // Convert lamports to SOL
                payout_amount: result.payout as f64 / 1_000_000_000.0,
                settlement_tx_id: None,
            },
            vrf: VRFBundle {
                vrf_output: hex::encode(&result.vrf_output),
                vrf_proof: hex::encode(&result.vrf_proof),
                public_key: hex::encode(state.game_processor.get_public_key()),
                input_message: result.vrf_input_message.clone(),
            },
            outcome: result.outcome,
            timestamp: result.timestamp,
            game_data: GameData::CoinFlip {
                player_choice: result.player_choice,
                result_choice: match result.outcome {
                    crate::games::types::GameOutcome::Win => result.player_choice,
                    crate::games::types::GameOutcome::Loss => {
                        // Return opposite choice
                        match result.player_choice {
                            CoinChoice::Heads => CoinChoice::Tails,
                            CoinChoice::Tails => CoinChoice::Heads,
                        }
                    },
                },
            },
            metadata: Some(serde_json::json!({
                "block_height": result.block_height,
                "block_hash": hex::encode(result.block_hash),
                "finalization_confirmed": true
            })),
        };
        
        Ok(Json(GameResponse::Complete {
            game_id,
            result: game_result,
        }))
    } else {
        // Game not found - might still be pending
        Ok(Json(GameResponse::Pending {
            game_id,
            message: Some("Game result not yet available. Transaction may still be processing.".to_string()),
        }))
    }
}

/// Verify VRF proof with detailed cryptographic validation
/// POST /api/verify/vrf
pub async fn verify_vrf(
    State(state): State<GameApiState>,
    Json(request): Json<VerifyVRFRequest>,
) -> Result<Json<VerifyVRFResponse>, (StatusCode, String)> {
    // Decode VRF proof and output from hex
    let vrf_proof = hex::decode(&request.vrf_proof)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid VRF proof hex: {}", e)))?;

    let vrf_output = hex::decode(&request.vrf_output)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid VRF output hex: {}", e)))?;

    // Validate VRF proof and output lengths
    if vrf_proof.len() != 64 {
        return Ok(Json(VerifyVRFResponse {
            is_valid: false,
            error: Some(format!(
                "Invalid VRF proof length: expected 64 bytes, got {}",
                vrf_proof.len()
            )),
            computed_result: None,
            explanation: Some("VRF proof must be exactly 64 bytes (512 bits) for Schnorrkel".to_string()),
        }));
    }

    if vrf_output.len() != 32 {
        return Ok(Json(VerifyVRFResponse {
            is_valid: false,
            error: Some(format!(
                "Invalid VRF output length: expected 32 bytes, got {}",
                vrf_output.len()
            )),
            computed_result: None,
            explanation: Some("VRF output must be exactly 32 bytes (256 bits)".to_string()),
        }));
    }

    // Verify cryptographically from request data, but do NOT trust a caller-supplied public key.
    // Pin verification to the node's configured VRF key.
    let vrf_bundle = crate::games::types::VRFBundle {
        vrf_output: request.vrf_output.clone(),
        vrf_proof: request.vrf_proof.clone(),
        public_key: hex::encode(state.game_processor.get_public_key()),
        input_message: request.input_message.clone(),
    };

    match crate::games::vrf_engine::VRFGameEngine::verify_vrf_proof(&vrf_bundle, &request.input_message) {
        Ok(is_valid) => {
            let computed = if request.game_type == crate::games::types::GameType::CoinFlip {
                let result_choice = crate::games::vrf_engine::VRFGameEngine::compute_coinflip(&vrf_output);
                Some(serde_json::json!({
                    "coinflip_result": format!("{:?}", result_choice),
                    "proof_length": vrf_proof.len(),
                    "output_length": vrf_output.len(),
                }))
            } else {
                Some(serde_json::json!({
                    "proof_length": vrf_proof.len(),
                    "output_length": vrf_output.len(),
                }))
            };

            Ok(Json(VerifyVRFResponse {
                is_valid,
                error: if is_valid { None } else { Some("VRF verification failed".to_string()) },
                computed_result: computed,
                explanation: None,
            }))
        }
        Err(e) => Ok(Json(VerifyVRFResponse {
            is_valid: false,
            error: Some(e),
            computed_result: None,
            explanation: None,
        })),
    }
}

/// Verify game by ID (includes full VRF verification)
/// GET /api/verify/game/:id
pub async fn verify_game_by_id(
    Path(game_id): Path<String>,
    State(state): State<GameApiState>,
) -> Result<Json<VerifyVRFResponse>, (StatusCode, String)> {
    // Extract transaction ID
    let tx_id = game_id.strip_prefix("tx-")
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Invalid game ID format".to_string()))?;
    
    // Query game result from DB-backed game processor
    let result = state
        .game_processor
        .get_game_result(tx_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Game {} not found", game_id)))?;

    // Strict inclusion cross-check: the stored game result must match canonical DB inclusion.
    // This prevents verifying a result that isn't actually anchored to the finalized chain.
    let api_storage = ApiStorage::new(state.storage.clone());
    let inclusion = match api_storage.find_transaction(tx_id) {
        Ok(Some((included_height, _idx, included_tx))) => {
            if included_tx.id != tx_id {
                return Ok(Json(VerifyVRFResponse {
                    is_valid: false,
                    error: Some("Transaction index mismatch (wrong tx at indexed location)".to_string()),
                    computed_result: None,
                    explanation: Some("The tx index points to a different transaction; inclusion proof is invalid.".to_string()),
                }));
            }

            let Some(block) = api_storage
                .get_block_by_height(included_height)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Storage error: {}", e)))?
            else {
                return Ok(Json(VerifyVRFResponse {
                    is_valid: false,
                    error: Some("Indexed block not found".to_string()),
                    computed_result: None,
                    explanation: Some("The tx index references a missing block; inclusion proof is invalid.".to_string()),
                }));
            };

            Some((included_height, block.block_hash))
        }
        Ok(None) => None,
        Err(e) => {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Storage error: {}", e)));
        }
    };

    let Some((included_height, included_hash)) = inclusion else {
        return Ok(Json(VerifyVRFResponse {
            is_valid: false,
            error: Some("Transaction not found in canonical chain".to_string()),
            computed_result: None,
            explanation: Some("No finalized transaction inclusion record exists for this tx_id.".to_string()),
        }));
    };

    if included_height != result.block_height || included_hash != result.block_hash {
        return Ok(Json(VerifyVRFResponse {
            is_valid: false,
            error: Some("Stored result inclusion does not match canonical chain".to_string()),
            computed_result: Some(serde_json::json!({
                "stored_block_height": result.block_height,
                "stored_block_hash": hex::encode(result.block_hash),
                "canonical_block_height": included_height,
                "canonical_block_hash": hex::encode(included_hash),
            })),
            explanation: Some("The persisted game result is not anchored to the canonical inclusion record.".to_string()),
        }));
    }

    match state.game_processor.verify_game_result(&result) {
        Ok(true) => Ok(Json(VerifyVRFResponse {
            is_valid: true,
            error: None,
            computed_result: Some(serde_json::json!({
                "game_id": game_id,
                "outcome": format!("{:?}", result.outcome),
                "payout": result.payout,
                "block_height": result.block_height,
                "block_hash": hex::encode(result.block_hash),
                "vrf_input_message": result.vrf_input_message,
            })),
            explanation: Some(format!(
                "Game result verified for block height {} (inclusion + VRF + game logic OK).",
                result.block_height
            )),
        })),
        Ok(false) => Ok(Json(VerifyVRFResponse {
            is_valid: false,
            error: Some("Verification failed".to_string()),
            computed_result: None,
            explanation: None,
        })),
        Err(e) => Ok(Json(VerifyVRFResponse {
            is_valid: false,
            error: Some(format!("Verification error: {}", e)),
            computed_result: None,
            explanation: None,
        })),
    }
}

/// List supported tokens
/// GET /api/tokens
pub async fn list_supported_tokens() -> Json<Vec<Token>> {
    Json(Token::all_supported())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_blockchain_storage_with_tx(
        storage: &OptimizedStorage,
        height: u64,
        tx: crate::Transaction,
    ) -> crate::Block {
        let block = crate::Block::new(height, [0u8; 32], vec![tx], [0u8; 32]);
        let key = format!("block:height:{}", height);
        let bytes = bincode::serialize(&block).unwrap();
        storage.put(key.as_bytes(), &bytes).unwrap();
        block
    }

    #[tokio::test]
    async fn test_verify_game_inclusion_mismatch_fails() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(OptimizedStorage::new(dir.path()).unwrap());

        // Canonical block at height 10 with tx_id=1
        let canonical_tx = crate::Transaction {
            id: 1,
            sender: [0u8; 32],
            data: vec![1, 2, 3],
            nonce: 1,
            timestamp: 1234567890000,
        };
        let canonical_block = build_blockchain_storage_with_tx(storage.as_ref(), 10, canonical_tx);

        // Store tx index (initially correct)
        storage.put(b"latest_height", &11u64.to_le_bytes()).unwrap();
        storage
            .put(format!("tx_idx:{}", 1).as_bytes(), b"10:0")
            .unwrap();

        // Create a different block at height 11 with a different tx at index 0
        let wrong_tx = crate::Transaction {
            id: 999,
            sender: [9u8; 32],
            data: vec![9, 9, 9],
            nonce: 1,
            timestamp: 1234567890001,
        };
        let _wrong_block = build_blockchain_storage_with_tx(storage.as_ref(), 11, wrong_tx);

        // Corrupt tx index to point to the wrong location
        storage
            .put(format!("tx_idx:{}", 1).as_bytes(), b"11:0")
            .unwrap();

        // Persist a game result that claims inclusion in the canonical block (height 10)
        let processor = Arc::new(BlockchainGameProcessor::new_with_persistent_key(storage.clone()).unwrap());

        let bet_data = GameBetData {
            game_type: GameType::CoinFlip,
            bet_amount: 1000,
            token: Token::sol(),
            player_choice: crate::games::types::CoinChoice::Heads,
            player_address: "test_player".to_string(),
        };
        let common_tx = Transaction::new_game_bet_with_timestamp(
            1,
            [0u8; 32],
            serde_json::to_vec(&bet_data).unwrap(),
            1,
            1234567890000,
        );
        processor
            .process_game_transaction(&common_tx, canonical_block.block_hash, canonical_block.height)
            .unwrap();

        let (tx_sender, _rx) = tokio::sync::mpsc::channel::<Transaction>(1);
        let state = GameApiState {
            storage: storage.clone(),
            game_processor: processor,
            tx_sender,
            finalization_waiter: None,
            fairness_waiter: None,
        };

        let response = verify_game_by_id(Path("tx-1".to_string()), State(state))
            .await
            .unwrap();
        assert!(!response.0.is_valid);
    }

    #[tokio::test]
    async fn test_play_coinflip_queue_full_returns_503() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(OptimizedStorage::new(dir.path()).unwrap());
        let processor = Arc::new(BlockchainGameProcessor::new_with_persistent_key(storage.clone()).unwrap());

        let (tx_sender, _rx) = tokio::sync::mpsc::channel::<Transaction>(1);
        // Fill the queue
        let dummy_tx = Transaction::new_game_bet(42, [0u8; 32], vec![0u8; 1], 1);
        tx_sender.try_send(dummy_tx).unwrap();

        let state = GameApiState {
            storage,
            game_processor: processor,
            tx_sender,
            finalization_waiter: None,
            fairness_waiter: None,
        };

        let request = CoinFlipPlayRequest {
            player_id: "test_player".to_string(),
            bet_amount: 1.0,
            choice: crate::games::types::CoinChoice::Heads,
            token: Token::sol(),
            wallet_signature: None,
        };

        let err = play_coinflip(State(state), Json(request)).await.err().unwrap();
        assert_eq!(err.0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_list_supported_tokens() {
        let tokens = list_supported_tokens().await;
        assert!(tokens.0.len() >= 3); // SOL, USDC, USDT
        assert!(tokens.0.iter().any(|t| t.symbol == "SOL"));
        assert!(tokens.0.iter().any(|t| t.symbol == "USDC"));
    }
}
