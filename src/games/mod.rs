//! Casino Games Module
//! 
//! Core gaming functionality with clean separation of concerns.
//! Designed for easy extension with new game types and settlement systems.

pub mod types;
pub mod vrf_engine;
pub mod pending_pool;
pub mod processor;
// pub mod settlement;
// pub mod test_framework;
// pub mod api_endpoints;

pub use types::*;
pub use vrf_engine::VRFGameEngine;
pub use pending_pool::PendingGamesPool;
pub use processor::GameProcessor;
// pub use settlement::*;
// pub use test_framework::*;
// pub use api_endpoints::*;

// Module organization summary for developers:
// 
// 1. `types` - Core data structures (Token, GameType, etc.)
// 2. `vrf_engine` - Provably fair random number generation
// 3. `processor` - Main game processing logic
// 4. `settlement` - Payment and settlement systems
// 5. `test_framework` - Comprehensive testing infrastructure
// 6. `api_endpoints` - Clean API design patterns
// 7. `pending_pool` - Game state management
// 
// To add a new game:
// 1. Add game type to `types.rs`
// 2. Implement game logic in `processor.rs`
// 3. Create API endpoint in `api_endpoints.rs`
// 4. Add test scenarios in `test_framework.rs`
// 5. Update settlement logic if needed
