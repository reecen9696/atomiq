#!/bin/bash

echo "==========================================="
echo "Testing Blockchain Startup Modes"
echo "==========================================="
echo ""

echo "1️⃣  Testing Mode (high-performance benchmark)"
echo "   Should show: ⚠️  Testing mode: Clearing database"
echo "-------------------------------------------"
cargo run --release --bin atomiq-unified -- benchmark-performance --target-tps 1000 --total-transactions 10 --concurrent-submitters 1 2>&1 | grep -E "(Testing mode|Production mode|Starting High-Performance)" | head -3
echo ""

echo "2️⃣  Production Mode (single-validator)"  
echo "   Should show: 📦 Production mode: Preserving existing blockchain data"
echo "-------------------------------------------"
cargo run --release --bin atomiq-unified -- single-validator --max-tx-per-block 5 --block-time-ms 2000 2>&1 | grep -E "(Testing mode|Production mode|Starting Single)" | head -3
echo ""

echo "3️⃣  Production Mode (throughput-test uses default config)"
echo "   Should show: 📦 Production mode: Preserving existing blockchain data"
echo "-------------------------------------------"
cargo run --release --bin atomiq-unified -- throughput-test --total-transactions 10 --batch-size 5 2>&1 | grep -E "(Testing mode|Production mode|Starting Throughput)" | head -3
echo ""

echo "==========================================="
echo "✅ Test complete! Check the output above."
echo "==========================================="
