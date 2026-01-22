//! Casino Games API - Blockchain-Integrated Edition
//!
//! This module provides HTTP endpoints for provably fair casino games.
//! ALL game outcomes are generated ON-CHAIN by the blockchain's VRF engine,
//! ensuring verifiable fairness and preventing cherry-picking attacks.

use crate::{
    blockchain_game_processor::{BlockchainGameProcessor, GameBetData, BlockchainGameResult},
    common::types::{Transaction, TransactionType},
    games::{
        types::{CoinFlipPlayRequest, GameResponse, Token, VerifyVRFRequest, VerifyVRFResponse, GameType},
    },
    storage::OptimizedStorage,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
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
}

/// Play coin flip game by submitting a transaction to the blockchain
/// POST /api/coinflip/play
pub async fn play_coinflip(
    State(state): State<GameApiState>,
    Json(request): Json<CoinFlipPlayRequest>,
) -> Result<Json<GameResponse>, (StatusCode, String)> {
    // Create game bet data
    let bet_data = GameBetData {
        game_type: GameType::CoinFlip,
        bet_amount: request.bet_amount,
        token: request.token,
        player_choice: request.player_choice,
        player_address: request.player_address.clone(),
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
    
    // Submit transaction to blockchain
    state.tx_sender.send(transaction.clone())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to submit transaction: {}", e)))?;
    
    // Return transaction ID for polling
    Ok(Json(GameResponse::Pending {
        game_id: format!("tx-{}", tx_id),
        message: Some("Game transaction submitted to blockchain. Poll /api/game/tx-{id} for result.".to_string()),
    }))
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
        // Convert BlockchainGameResult to GameResult
        let game_result = crate::games::types::GameResult {
            game_id: game_id.clone(),
            game_type: result.game_type,
            player_address: result.player_address,
            bet_amount: result.bet_amount,
            token: result.token,
            outcome: result.outcome,
            payout: result.payout,
            player_choice: result.player_choice,
            vrf_proof: hex::encode(&result.vrf_proof),
            vrf_output: hex::encode(&result.vrf_output),
            timestamp: result.timestamp,
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

/// Verify VRF proof
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
    
    // Create a BlockchainGameResult for verification
    // In production, this would query from blockchain storage
    let dummy_result = BlockchainGameResult {
        transaction_id: 0,
        player_address: String::new(),
        game_type: request.game_type,
        bet_amount: 0,
        token: crate::games::types::Token::Sol { decimals: 9 },
        player_choice: crate::games::types::CoinChoice::Heads,
        outcome: crate::games::types::GameOutcome::Heads,
        coin_result: crate::games::types::CoinFlipResult::Heads,
        vrf_proof,
        vrf_output,
        vrf_input_message: String::new(),
        payout: 0,
        timestamp: 0,
        block_height: 0,
        block_hash: [0u8; 32],
        settlement_status: crate::blockchain_game_processor::SettlementStatus::PendingSettlement,
        version: 1,
        solana_tx_id: None,
        settlement_error: None,
        settlement_completed_at: None,
        retry_count: 0,
        next_retry_after: None,
    };
    
    // Verify the proof using blockchain's VRF engine
    match state.game_processor.verify_game_result(&dummy_result) {
        Ok(is_valid) => {
            if is_valid {
                Ok(Json(VerifyVRFResponse {
                    is_valid: true,
                    error: None,
                    computed_result: Some(serde_json::json!({
                        "message": "VRF proof is cryptographically valid",
                        "verified_by": "blockchain"
                    })),
                    explanation: Some("This VRF proof was generated by the blockchain and is cryptographically verifiable.".to_string()),
                }))
            } else {
                Ok(Json(VerifyVRFResponse {
                    is_valid: false,
                    error: Some("VRF proof verification failed".to_string()),
                    computed_result: None,
                    explanation: None,
                }))
            }
        }
        Err(e) => {
            Ok(Json(VerifyVRFResponse {
                is_valid: false,
                error: Some(format!("Verification error: {}", e)),
                computed_result: None,
                explanation: None,
            }))
        }
    }
}
        is_valid: true,
        error: None,
        computed_result: Some(computed_result),
        explanation,
    }))
}

/// Verify game by ID (includes full VRF verification)
/// GET /api/verify/game/:id
pub async fn verify_game_by_id(
    Path(game_id): Path<String>,
    State(_state): State<GameApiState>,
) -> Result<Json<VerifyVRFResponse>, (StatusCode, String)> {
    // TODO: Query game from blockchain storage
    // For now, return not found
    Err((
        StatusCode::NOT_FOUND,
        format!("Game {} not found", game_id),
    ))
}

/// List supported tokens
/// GET /api/tokens
pub async fn list_supported_tokens() -> Json<Vec<Token>> {
    Json(Token::all_supported())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::CoinChoice;

    #[tokio::test]
    async fn test_verify_vrf() {
        let vrf_engine = Arc::new(VRFGameEngine::new_random());
        let processor = Arc::new(GameProcessor::new(vrf_engine.clone()));
        let pending_pool = Arc::new(PendingGamesPool::new());

        let state = GameApiState {
            processor,
            pending_pool,
            vrf_engine: vrf_engine.clone(),
        };

        // Generate a valid VRF proof
        let vrf_bundle = vrf_engine
            .generate_outcome("test-game", GameType::CoinFlip, "player-1", "heads")
            .unwrap();

        let request = VerifyVRFRequest {
            vrf_output: vrf_bundle.vrf_output,
            vrf_proof: vrf_bundle.vrf_proof,
            public_key: vrf_bundle.public_key,
            input_message: vrf_bundle.input_message,
            game_type: GameType::CoinFlip,
        };

        let response = verify_vrf(State(state), Json(request)).await.unwrap();
        assert!(response.0.is_valid);
        assert!(response.0.computed_result.is_some());
    }

    #[tokio::test]
    async fn test_list_supported_tokens() {
        let tokens = list_supported_tokens().await;
        assert!(tokens.0.len() >= 3); // SOL, USDC, USDT
        assert!(tokens.0.iter().any(|t| t.symbol == "SOL"));
        assert!(tokens.0.iter().any(|t| t.symbol == "USDC"));
    }
}
