//! Casino Games API - Blockchain-Integrated Edition
//!
//! This module provides HTTP endpoints for provably fair casino games.
//! ALL game outcomes are generated ON-CHAIN by the blockchain's VRF engine,
//! ensuring verifiable fairness and preventing cherry-picking attacks.

use crate::{
    blockchain_game_processor::{BlockchainGameProcessor, GameBetData, BlockchainGameResult},
    common::types::{Transaction, TransactionType},
    fairness::{FairnessError, FairnessWaiter},
    games::{
        types::{CoinFlipPlayRequest, GameResponse, Token, VerifyVRFRequest, VerifyVRFResponse, GameType},
    },
    storage::OptimizedStorage,
    finalization::FinalizationWaiter,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use sha2::{Sha256, Digest};
use std::sync::Arc;

/// Shared state for game API
#[derive(Clone)]
pub struct GameApiState {
    /// Blockchain storage for querying game results
    pub storage: Arc<OptimizedStorage>,
    
    /// Blockchain game processor (for verification and querying)
    pub game_processor: Arc<BlockchainGameProcessor>,
    
    /// Transaction submission channel (to blockchain)
    pub tx_sender: tokio::sync::mpsc::UnboundedSender<Transaction>,
    
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
    state.tx_sender.send(transaction.clone())
        .map_err(|e| {
            tracing::error!("Failed to submit transaction {}: {}", tx_id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to submit transaction: {}", e))
        })?;
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

    // Verify cryptographically from request data (no in-memory state).
    let vrf_bundle = crate::games::types::VRFBundle {
        vrf_output: request.vrf_output.clone(),
        vrf_proof: request.vrf_proof.clone(),
        public_key: request.public_key.clone(),
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
                "Game result verified for block height {} (VRF + game logic OK).",
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

    #[tokio::test]
    async fn test_list_supported_tokens() {
        let tokens = list_supported_tokens().await;
        assert!(tokens.0.len() >= 3); // SOL, USDC, USDT
        assert!(tokens.0.iter().any(|t| t.symbol == "SOL"));
        assert!(tokens.0.iter().any(|t| t.symbol == "USDC"));
    }
}
