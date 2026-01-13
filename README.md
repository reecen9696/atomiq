# Atomiq Blockchain

High-performance single-validator blockchain using HotStuff-rs consensus, optimized for maximum throughput with cryptographic integrity.

## Features

- 🚀 **High Throughput**: Target 20,000+ TPS with proper validation
- ⚡ **Fast Block Times**: 5-10ms block creation target
- 🔒 **Single Validator BFT**: Simplified Byzantine fault tolerant consensus with one validator
- 🔗 **Cryptographic Hashing**: True blockchain integrity with SHA-256 block hashing
- 📊 **Real-time Metrics**: Comprehensive performance monitoring
- 🧹 **Lean Design**: Minimal complexity, maximum performance

## Quick Start

```bash
# Run main blockchain (single validator HotStuff)
cargo run --release

# Run simplified version (bypasses consensus)
cargo run --bin simple_blockchain --release

# Run explicit single validator test
cargo run --bin dual_validator --release

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

## Performance Characteristics

- **Block Creation**: ~2-5ms with cryptographic hashing
- **Hash Computation**: ~2 microseconds (SHA-256)
- **Transaction Validation**: Full nonce + state validation enabled
- **Storage Writes**: Batched RocksDB operations
- **Network Overhead**: Zero (single validator, mock network)
- **Consensus Overhead**: Minimal (self-messaging only)

## Architecture

- **Consensus**: Single validator HotStuff-rs BFT (Power = 1)
- **Storage**: RocksDB with LZ4 compression and optimized write buffers
- **Networking**: Mock network for single-node operation (no P2P overhead)
- **State**: In-memory HashMap with persistent RocksDB backing
- **Block Hashing**: SHA-256 cryptographic hashing (height + justify + data_hash)
- **View Timeout**: Aggressive 10ms for single validator optimization

## Single Validator Setup

The blockchain is currently configured as a single validator system:

```rust
// Single validator with full voting power
validator_set_updates.insert(verifying_key, Power::new(1));

// Optimized timing for single validator
.max_view_time(Duration::from_millis(10))
.block_sync_trigger_min_view_difference(1)
```

This eliminates multi-validator consensus overhead while maintaining:

- ✅ Cryptographic block integrity
- ✅ Persistent blockchain storage
- ✅ HotStuff consensus structure
- ✅ Transaction validation
- ❌ No Byzantine fault tolerance (single point of failure)
- ❌ No network consensus delays

## Expected Database Growth

With 15,000 TPS average:

- **Weekly Growth**: ~3-4 TB
- **With Pruning**: ~1.2 TB (30-day retention)
- **Transaction Size**: ~166 bytes average
- **State Per TX**: ~148 bytes average

## Future Extensibility

This single-validator implementation can be easily extended to:

- **Multi-validator networks**: Add more validators to the ValidatorSet
- **Real P2P networking**: Replace MockNetwork with TCP/UDP implementation
- **Casino/gaming logic**: Add application-specific transaction types
- **Advanced state management**: Implement complex state machines
- **Cross-chain features**: Bridge to other blockchains

## Performance vs Security Trade-offs

| Aspect                    | Single Validator | Multi-Validator BFT |
| ------------------------- | ---------------- | ------------------- |
| TPS                       | 20,000+          | ~1,000-5,000        |
| Block Time                | 5-10ms           | 1-3 seconds         |
| Network Overhead          | None             | High                |
| Byzantine Fault Tolerance | ❌               | ✅                  |
| Cryptographic Integrity   | ✅               | ✅                  |
| Production Ready          | POC Only         | Yes                 |

# atomiq
