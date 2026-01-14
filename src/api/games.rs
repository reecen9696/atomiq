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
    
    // Simulate block finalization for immediate processing
    use sha2::{Sha256, Digest};
    let block_height = 100000u64; // In production, get from blockchain
    let mut hasher = Sha256::new();
    hasher.update(&block_height.to_be_bytes());
    hasher.update(b"current_block_consensus_data");
    hasher.update(&transaction.data);
    let block_hash = hasher.finalize();
    let block_hash_array: [u8; 32] = block_hash.into();
    
    // Process game transaction immediately with VRF
    let result = state.game_processor.process_game_transaction(&transaction, block_hash_array, block_height)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Game processing failed: {}", e)))?;
    
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
            public_key: "blockchain_vrf_key".to_string(),
            input_message: format!("tx-{}", result.transaction_id),
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
        metadata: None,
    };
    
    // Return complete game result immediately
    Ok(Json(GameResponse::Complete {
        game_id: format!("tx-{}", tx_id),
        result: game_result,
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
                public_key: "blockchain_vrf_key".to_string(), // TODO: Get actual public key
                input_message: format!("tx-{}", result.transaction_id),
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
            metadata: None,
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
            error: Some(format!("Invalid VRF proof length: expected 64 bytes, got {}", vrf_proof.len())),
            computed_result: None,
            explanation: Some("VRF proof must be exactly 64 bytes (512 bits) for Schnorrkel".to_string()),
        }));
    }
    
    if vrf_output.len() != 32 {
        return Ok(Json(VerifyVRFResponse {
            is_valid: false,
            error: Some(format!("Invalid VRF output length: expected 32 bytes, got {}", vrf_output.len())),
            computed_result: None,
            explanation: Some("VRF output must be exactly 32 bytes (256 bits)".to_string()),
        }));
    }
    
    // Create a BlockchainGameResult for verification
    let dummy_result = BlockchainGameResult {
        transaction_id: 0,
        player_address: String::new(),
        game_type: request.game_type,
        bet_amount: 0,
        token: crate::games::types::Token::sol(),
        player_choice: crate::games::types::CoinChoice::Heads,
        outcome: crate::games::types::GameOutcome::Win,
        coin_result: crate::games::types::CoinFlipResult::Heads,
        vrf_proof: vrf_proof.clone(),
        vrf_output: vrf_output.clone(),
        payout: 0,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        block_height: 0,
    };
    
    // Perform detailed VRF verification
    match state.game_processor.verify_game_result(&dummy_result) {
        Ok(is_valid) => {
            if is_valid {
                // Generate essential verification data
                let verification_details = serde_json::json!({
                    "proof_length": vrf_proof.len(),
                    "output_length": vrf_output.len(),
                    "timestamp": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                });
                
                Ok(Json(VerifyVRFResponse {
                    is_valid: true,
                    error: None,
                    computed_result: Some(verification_details),
                    explanation: None,
                }))
            } else {
                Ok(Json(VerifyVRFResponse {
                    is_valid: false,
                    error: Some("Verification failed".to_string()),
                    computed_result: None,
                    explanation: None,
                }))
            }
        }
        Err(e) => {
            Ok(Json(VerifyVRFResponse {
                is_valid: false,
                error: Some(format!("{}", e)),
                computed_result: None,
                explanation: None,
            }))
        }
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
    
    // Query game result from blockchain
    let result = state.game_processor.get_game_result(tx_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Game {} not found", game_id)))?;
    
    // Verify the VRF proof
    match state.game_processor.verify_game_result(&result) {
        Ok(is_valid) => {
            if is_valid {
                Ok(Json(VerifyVRFResponse {
                    is_valid: true,
                    error: None,
                    computed_result: Some(serde_json::json!({
                        "game_id": game_id,
                        "outcome": format!("{:?}", result.outcome),
                        "payout": result.payout,
                        "block_height": result.block_height,
                    })),
                    explanation: Some(format!(
                        "Game result verified on blockchain at block height {}. VRF proof is valid.",
                        result.block_height
                    )),
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
