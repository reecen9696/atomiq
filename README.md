# Lean Blockchain

High-performance blockchain implementation using HotStuff-rs consensus, optimized for maximum throughput with proper validation.

## Features

- 🚀 **High Throughput**: Target 20,000+ TPS with proper validation
- ⚡ **Fast Block Times**: 5-10ms block creation target
- 🔒 **BFT Consensus**: Byzantine fault tolerant using HotStuff protocol
- 📊 **Real-time Metrics**: Comprehensive performance monitoring
- 🧹 **Lean Design**: Minimal complexity, maximum performance

## Quick Start

```bash
# Build and run
cargo run --release

# Run benchmarks
cargo bench

# Run tests
cargo test
```

## Configuration

The blockchain can be configured via `BlockchainConfig`:

```rust
BlockchainConfig {
    max_transactions_per_block: 10000, // High throughput batching
    max_block_time_ms: 10,             // 10ms block times
    enable_state_validation: true,     // Full validation for real TPS
    batch_size_threshold: 1000,        // Create block when we hit 1000 txs
}
```

## Performance Targets

- **TPS**: 20,000+ transactions per second
- **Block Time**: 5-10ms average
- **Latency**: Sub-millisecond transaction submission
- **Validation**: Full state validation enabled

## Architecture

- **Storage**: RocksDB with LZ4 compression
- **Consensus**: HotStuff-rs with single validator (expandable)
- **Networking**: Mock network (replaceable with real P2P)
- **State**: In-memory HashMap with persistent backing

## Expected Database Growth

With 15,000 TPS average:

- **Weekly Growth**: ~3-4 TB
- **With Pruning**: ~1.2 TB (30-day retention)
- **Transaction Size**: ~166 bytes average
- **State Per TX**: ~148 bytes average

## Future Extensibility

This implementation is designed to be easily extensible for:

- Casino/gaming logic
- Multi-validator networks
- Real P2P networking
- Advanced state management
- Cross-chain features
# atomiq
