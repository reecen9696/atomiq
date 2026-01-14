# Atomiq Blockchain

High-performance Byzantine Fault Tolerant (BFT) blockchain with Stage 2 optimizations.

## 📁 Project Structure

```
atomiq/
├── src/                    # Source code
│   ├── api/               # REST API and WebSocket server
│   ├── bin/               # Binary entry points
│   ├── common/            # Shared utilities
│   └── *.rs               # Core blockchain modules
├── tests/                 # Integration tests
├── examples/              # Usage examples
├── docs/                  # Documentation
│   ├── README.md          # Project overview
│   ├── REFACTORING_GUIDE.md
│   ├── REFACTORING_SUMMARY.md
│   ├── CLEAN_CODE_COMPLETE.md
│   └── STAGE2_TEST_REPORT.md
├── deployment/            # Deployment configurations
│   ├── docker/           # Docker files
│   │   ├── Dockerfile
│   │   └── docker-compose.yml
│   ├── nginx/            # Reverse proxy config
│   ├── certs/            # SSL certificates
│   └── monitoring/       # Prometheus config
├── scripts/              # Utility scripts
│   ├── test_all.sh      # Run all tests
│   ├── test_api.sh      # Test API endpoints
│   ├── test_modes.sh    # Test consensus modes
│   └── deploy.sh        # Deployment script
├── tools/                # Development tools
│   ├── check_keys.rs    # Key validation
│   └── inspect_keys.rs  # Key inspection
├── DB/                   # Database storage (gitignored)
├── logs/                 # Application logs (gitignored)
├── target/               # Build artifacts (gitignored)
├── Cargo.toml           # Rust dependencies
└── atomiq.toml          # Blockchain configuration
```

## 🚀 Quick Start

### Prerequisites

- Rust 1.75+
- RocksDB
- OpenSSL (for API TLS)

### Build

```bash
# Build all binaries
cargo build --release

# Build specific binary
cargo build --release --bin atomiq-unified
cargo build --release --bin atomiq-api
```

### Run

```bash
# Start blockchain with API
cargo run --release --bin atomiq-unified

# Start API server only
cargo run --release --bin atomiq-api -- --db-path ./DB/blockchain_data --port 8080

# Run benchmark
cargo run --release --bin atomiq-unified -- benchmark-performance \
  --target-tps 1000 \
  --total-transactions 10000 \
  --concurrent-submitters 4
```

### Test

```bash
# Run all tests
cargo test

# Run API integration tests
./scripts/test_api.sh

# Run consensus mode tests
./scripts/test_modes.sh
```

## 📖 Documentation

- [Main Documentation](docs/README.md) - Comprehensive project docs
- [Refactoring Guide](docs/REFACTORING_GUIDE.md) - Clean code principles
- [Stage 2 Report](docs/STAGE2_TEST_REPORT.md) - Performance optimizations
- [API Documentation](#) - Generate with `cargo doc --open`

## 🐳 Docker Deployment

```bash
# Build and run with Docker Compose
cd deployment/docker
docker-compose up -d

# With monitoring stack
docker-compose --profile monitoring up -d

# With nginx reverse proxy
docker-compose --profile production up -d
```

## 📊 Monitoring

- **Metrics**: http://localhost:8080/metrics (Prometheus format)
- **Health**: http://localhost:8080/health
- **Status**: http://localhost:8080/status

## 🏗️ Architecture

### Core Components

- **Blockchain Engine**: BFT consensus with HotStuff protocol
- **Transaction Pool**: Configurable ordering and capacity
- **Storage Layer**: RocksDB with optimizations
- **API Server**: REST + WebSocket with caching
- **Monitoring**: Prometheus metrics + real-time stats

### Stage 2 Features

- ✅ Lock-free storage operations
- ✅ LRU caching (blocks + transactions)
- ✅ WebSocket real-time updates
- ✅ Prometheus metrics
- ✅ Security middleware
- ✅ Load balancing ready

## 🔧 Configuration

Edit `atomiq.toml`:

```toml
[blockchain]
chain_id = 1
max_transactions_per_block = 1000

[storage]
data_directory = "./DB/blockchain_data"
compression_type = "Lz4"

[consensus]
mode = "DirectCommit"  # or "FullHotStuff"
```

## 🤝 Contributing

See [REFACTORING_GUIDE.md](docs/REFACTORING_GUIDE.md) for code standards.

## 📝 License

Apache 2.0

## 🎯 Performance

- **TPS**: 10,000+ transactions per second
- **Block Time**: <100ms average
- **API Response**: <1ms average
- **Memory**: Efficient RocksDB storage

## 📞 Support

- GitHub Issues: [Report bugs](https://github.com/yourorg/atomiq/issues)
- Documentation: `cargo doc --open`
