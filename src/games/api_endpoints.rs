//! Game API Endpoints Module
//! 
//! Extensible API design for casino games with clean separation of concerns.
//! Demonstrates how to easily add new game types and maintain consistency.

use crate::games::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Base API configuration
#[derive(Debug, Clone)]
pub struct GameApiConfig {
    pub enable_rate_limiting: bool,
    pub max_requests_per_second: usize,
    pub enable_cors: bool,
    pub require_authentication: bool,
    pub enable_metrics: bool,
}

impl Default for GameApiConfig {
    fn default() -> Self {
        Self {
            enable_rate_limiting: true,
            max_requests_per_second: 100,
            enable_cors: true,
            require_authentication: false,
            enable_metrics: true,
        }
    }
}

/// Generic game request wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct GameApiRequest<T> {
    pub player_id: String,
    pub game_data: T,
    pub metadata: RequestMetadata,
}

/// Request metadata for tracking and analytics
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestMetadata {
    pub client_id: Option<String>,
    pub session_id: Option<String>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub timestamp: u64,
}

/// Generic game response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct GameApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
    pub metadata: ResponseMetadata,
}

/// Response metadata for debugging and analytics
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMetadata {
    pub request_id: String,
    pub processing_time_ms: u64,
    pub server_timestamp: u64,
    pub rate_limit_remaining: Option<u32>,
}

/// API error structure
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Option<HashMap<String, String>>,
}

/// Trait for implementing game-specific endpoints
pub trait GameEndpoint<TRequest, TResponse> {
    /// Process the game request
    async fn process(&self, request: GameApiRequest<TRequest>) -> GameApiResponse<TResponse>;
    
    /// Validate the request before processing
    fn validate_request(&self, request: &GameApiRequest<TRequest>) -> Result<(), ApiError>;
    
    /// Get endpoint metadata
    fn get_metadata(&self) -> EndpointMetadata;
}

/// Endpoint metadata for documentation and routing
#[derive(Debug, Clone)]
pub struct EndpointMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub path: String,
    pub method: HttpMethod,
    pub rate_limit: Option<u32>,
    pub auth_required: bool,
}

#[derive(Debug, Clone)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
}

/// CoinFlip specific request/response types
#[derive(Debug, Serialize, Deserialize)]
pub struct CoinFlipApiRequest {
    pub choice: CoinChoice,
    pub bet_amount: f64,
    pub token: Token,
    pub wallet_signature: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CoinFlipApiResponse {
    pub game_id: String,
    pub result: CoinChoice,
    pub outcome: GameOutcome,
    pub payout: f64,
    pub vrf_proof: VRFProof,
}

/// CoinFlip endpoint implementation
pub struct CoinFlipEndpoint {
    game_processor: GameProcessor,
    config: GameApiConfig,
}

impl CoinFlipEndpoint {
    pub fn new(game_processor: GameProcessor, config: GameApiConfig) -> Self {
        Self {
            game_processor,
            config,
        }
    }
}

#[async_trait::async_trait]
impl GameEndpoint<CoinFlipApiRequest, CoinFlipApiResponse> for CoinFlipEndpoint {
    async fn process(&self, request: GameApiRequest<CoinFlipApiRequest>) -> GameApiResponse<CoinFlipApiResponse> {
        let start_time = std::time::Instant::now();
        let request_id = format!("req_{}", uuid::Uuid::new_v4());
        
        // Validate request
        if let Err(error) = self.validate_request(&request) {
            return GameApiResponse {
                success: false,
                data: None,
                error: Some(error),
                metadata: ResponseMetadata {
                    request_id,
                    processing_time_ms: start_time.elapsed().as_millis() as u64,
                    server_timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    rate_limit_remaining: Some(99), // TODO: Implement actual rate limiting
                },
            };
        }
        
        // Process the game
        let game_request = CoinFlipPlayRequest {
            player_id: request.player_id,
            choice: request.game_data.choice,
            bet_amount: request.game_data.bet_amount,
            token: request.game_data.token,
            wallet_signature: request.game_data.wallet_signature,
        };
        
        match self.game_processor.process_coinflip(game_request) {
            Ok(result) => {
                let response_data = CoinFlipApiResponse {
                    game_id: result.game_id,
                    result: match result.game_data {
                        GameData::CoinFlip { result_choice, .. } => result_choice,
                    },
                    outcome: result.outcome,
                    payout: result.payment.payout_amount,
                    vrf_proof: result.vrf,
                };
                
                GameApiResponse {
                    success: true,
                    data: Some(response_data),
                    error: None,
                    metadata: ResponseMetadata {
                        request_id,
                        processing_time_ms: start_time.elapsed().as_millis() as u64,
                        server_timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        rate_limit_remaining: Some(99),
                    },
                }
            },
            Err(e) => {
                GameApiResponse {
                    success: false,
                    data: None,
                    error: Some(ApiError {
                        code: "GAME_PROCESSING_ERROR".to_string(),
                        message: e.to_string(),
                        details: None,
                    }),
                    metadata: ResponseMetadata {
                        request_id,
                        processing_time_ms: start_time.elapsed().as_millis() as u64,
                        server_timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        rate_limit_remaining: Some(99),
                    },
                }
            }
        }
    }
    
    fn validate_request(&self, request: &GameApiRequest<CoinFlipApiRequest>) -> Result<(), ApiError> {
        // Validate player ID
        if request.player_id.is_empty() {
            return Err(ApiError {
                code: "INVALID_PLAYER_ID".to_string(),
                message: "Player ID cannot be empty".to_string(),
                details: None,
            });
        }
        
        // Validate bet amount
        if request.game_data.bet_amount <= 0.0 {
            return Err(ApiError {
                code: "INVALID_BET_AMOUNT".to_string(),
                message: "Bet amount must be positive".to_string(),
                details: Some([("min_bet".to_string(), "0.01".to_string())].into()),
            });
        }
        
        // Validate maximum bet
        if request.game_data.bet_amount > 1000.0 {
            return Err(ApiError {
                code: "BET_AMOUNT_TOO_HIGH".to_string(),
                message: "Bet amount exceeds maximum allowed".to_string(),
                details: Some([("max_bet".to_string(), "1000.0".to_string())].into()),
            });
        }
        
        Ok(())
    }
    
    fn get_metadata(&self) -> EndpointMetadata {
        EndpointMetadata {
            name: "coinflip".to_string(),
            version: "v1".to_string(),
            description: "Play coinflip game with provably fair VRF".to_string(),
            path: "/api/v1/games/coinflip/play".to_string(),
            method: HttpMethod::POST,
            rate_limit: Some(60), // 60 requests per minute
            auth_required: false,
        }
    }
}

/// Example of how to add a new game type: Dice
#[derive(Debug, Serialize, Deserialize)]
pub struct DiceApiRequest {
    pub prediction: u8, // 1-6
    pub bet_amount: f64,
    pub token: Token,
    pub wallet_signature: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiceApiResponse {
    pub game_id: String,
    pub predicted_number: u8,
    pub rolled_number: u8,
    pub outcome: GameOutcome,
    pub payout: f64,
    pub vrf_proof: VRFProof,
}

/// Dice endpoint implementation (example of extensibility)
pub struct DiceEndpoint {
    game_processor: GameProcessor, // Would need to be extended to support dice
    config: GameApiConfig,
}

impl DiceEndpoint {
    pub fn new(game_processor: GameProcessor, config: GameApiConfig) -> Self {
        Self {
            game_processor,
            config,
        }
    }
}

// This would implement GameEndpoint<DiceApiRequest, DiceApiResponse> when dice game is added

/// Game router for managing multiple endpoints
pub struct GameApiRouter {
    endpoints: HashMap<String, Box<dyn GameEndpointWrapper>>,
    config: GameApiConfig,
}

// Wrapper trait to handle different endpoint types
pub trait GameEndpointWrapper: Send + Sync {
    fn get_metadata(&self) -> EndpointMetadata;
    async fn handle_request(&self, request_body: &str) -> String;
}

impl GameApiRouter {
    pub fn new(config: GameApiConfig) -> Self {
        Self {
            endpoints: HashMap::new(),
            config,
        }
    }
    
    /// Register a new game endpoint
    pub fn register_endpoint(&mut self, endpoint: Box<dyn GameEndpointWrapper>) {
        let metadata = endpoint.get_metadata();
        self.endpoints.insert(metadata.name.clone(), endpoint);
    }
    
    /// Get available endpoints
    pub fn get_endpoints(&self) -> Vec<EndpointMetadata> {
        self.endpoints.values().map(|e| e.get_metadata()).collect()
    }
    
    /// Route request to appropriate endpoint
    pub async fn route_request(&self, game_type: &str, request_body: &str) -> String {
        match self.endpoints.get(game_type) {
            Some(endpoint) => endpoint.handle_request(request_body).await,
            None => {
                let error_response = GameApiResponse::<()> {
                    success: false,
                    data: None,
                    error: Some(ApiError {
                        code: "UNSUPPORTED_GAME".to_string(),
                        message: format!("Game type '{}' not supported", game_type),
                        details: None,
                    }),
                    metadata: ResponseMetadata {
                        request_id: "error".to_string(),
                        processing_time_ms: 0,
                        server_timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        rate_limit_remaining: None,
                    },
                };
                serde_json::to_string(&error_response).unwrap_or_default()
            }
        }
    }
    
    /// Generate API documentation
    pub fn generate_api_docs(&self) -> ApiDocumentation {
        ApiDocumentation {
            title: "HotStuff Casino API".to_string(),
            version: "1.0.0".to_string(),
            description: "Provably fair casino games with VRF verification".to_string(),
            endpoints: self.get_endpoints(),
        }
    }
}

/// API documentation structure
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiDocumentation {
    pub title: String,
    pub version: String,
    pub description: String,
    pub endpoints: Vec<EndpointMetadata>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_api_config_defaults() {
        let config = GameApiConfig::default();
        assert!(config.enable_rate_limiting);
        assert_eq!(config.max_requests_per_second, 100);
        assert!(config.enable_cors);
    }
    
    #[test]
    fn test_endpoint_metadata() {
        let game_processor = GameProcessor::new(VRFGameEngine::new_random());
        let config = GameApiConfig::default();
        let endpoint = CoinFlipEndpoint::new(game_processor, config);
        
        let metadata = endpoint.get_metadata();
        assert_eq!(metadata.name, "coinflip");
        assert_eq!(metadata.version, "v1");
        assert_eq!(metadata.path, "/api/v1/games/coinflip/play");
    }
    
    #[tokio::test]
    async fn test_validation_error() {
        let game_processor = GameProcessor::new(VRFGameEngine::new_random());
        let config = GameApiConfig::default();
        let endpoint = CoinFlipEndpoint::new(game_processor, config);
        
        let request = GameApiRequest {
            player_id: "".to_string(), // Invalid empty player ID
            game_data: CoinFlipApiRequest {
                choice: CoinChoice::Heads,
                bet_amount: 1.0,
                token: Token::sol(),
                wallet_signature: None,
            },
            metadata: RequestMetadata {
                client_id: None,
                session_id: None,
                user_agent: None,
                ip_address: None,
                timestamp: 0,
            },
        };
        
        let response = endpoint.process(request).await;
        assert!(!response.success);
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, "INVALID_PLAYER_ID");
    }
}