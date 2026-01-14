#!/bin/bash
# Comprehensive API Test Script

set -e

API_URL="${API_URL:-http://localhost:8080}"
PASS="\033[0;32m✓\033[0m"
FAIL="\033[0;31m✗\033[0m"

echo "======================================"
echo "  Atomiq API Comprehensive Test Suite"
echo "======================================"
echo ""

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Missing required command: $cmd" 1>&2
        exit 1
    fi
}

require_cmd curl
require_cmd jq

# Function to test an endpoint
test_endpoint() {
    local name="$1"
    local endpoint="$2"
    local expected_status="${3:-200}"
    
    echo -n "Testing $name... "
    
    status=$(curl -s -o /dev/null -w "%{http_code}" "$API_URL$endpoint")
    
    if [ "$status" -eq "$expected_status" ]; then
        echo -e "$PASS (HTTP $status)"
        return 0
    else
        echo -e "$FAIL (Expected HTTP $expected_status, got $status)"
        return 1
    fi
}

# Function to test JSON response
test_json_endpoint() {
    local name="$1"
    local endpoint="$2"
    local field="$3"
    
    echo -n "Testing $name... "
    
    response=$(curl -s "$API_URL$endpoint")
    value=$(echo "$response" | jq -r "$field" 2>/dev/null)
    
    if [ "$value" != "null" ] && [ -n "$value" ]; then
        echo -e "$PASS ($field = $value)"
        return 0
    else
        echo -e "$FAIL (Field '$field' not found or null)"
        echo "Response: $response"
        return 1
    fi
}

echo "=== Health & Status Endpoints ==="
test_endpoint "Health Check" "/health"
test_json_endpoint "Status - Node Info" "/status" ".node_info.id"
test_json_endpoint "Status - Latest Block" "/status" ".sync_info.latest_block_height"
echo ""

echo "=== Seed Chain (Create 1 Game TX) ==="
echo -n "Creating coinflip transaction... "

GAME_RESPONSE=$(curl -s -X POST "$API_URL/api/coinflip/play" \
  -H "Content-Type: application/json" \
  -d "{\
    \"player_id\": \"api-test\",\
    \"choice\": \"heads\",\
    \"token\": {\"symbol\": \"SOL\"},\
    \"bet_amount\": 1.0\
  }")

GAME_STATUS=$(echo "$GAME_RESPONSE" | jq -r '.status')
GAME_ID=$(echo "$GAME_RESPONSE" | jq -r '.game_id')

if [ "$GAME_STATUS" = "pending" ]; then
    echo "pending (polling /api/game/$GAME_ID)"
    for i in {1..10}; do
        sleep 1
        GAME_RESPONSE=$(curl -s "$API_URL/api/game/$GAME_ID")
        GAME_STATUS=$(echo "$GAME_RESPONSE" | jq -r '.status')
        if [ "$GAME_STATUS" = "complete" ]; then
            break
        fi
    done
fi

if [ "$GAME_STATUS" != "complete" ]; then
    echo -e "$FAIL (game never completed)"
    echo "Response: $GAME_RESPONSE"
    exit 1
fi

TX_ID=$(echo "$GAME_ID" | sed 's/^tx-//')
BLOCK_HEIGHT=$(echo "$GAME_RESPONSE" | jq -r '.result.block_height')

if [ -z "$TX_ID" ] || [ "$TX_ID" = "null" ]; then
    echo -e "$FAIL (missing tx id)"
    echo "Response: $GAME_RESPONSE"
    exit 1
fi

echo -e "$PASS (tx_id=$TX_ID, block_height=$BLOCK_HEIGHT)"
echo ""

echo "=== Block Endpoints ==="
test_endpoint "Block List" "/blocks"
test_json_endpoint "Block List - Pagination" "/blocks?limit=5" ".pagination.total_returned"

if [ "$BLOCK_HEIGHT" != "null" ] && [ -n "$BLOCK_HEIGHT" ]; then
    test_endpoint "Block Detail (Height $BLOCK_HEIGHT)" "/block/$BLOCK_HEIGHT"
fi

LATEST_HEIGHT=$(curl -s "$API_URL/status" | jq -r '.sync_info.latest_block_height')
if [ "$LATEST_HEIGHT" != "null" ] && [ -n "$LATEST_HEIGHT" ]; then
    NOT_FOUND_HEIGHT=$((LATEST_HEIGHT + 9999))
    test_endpoint "Block Not Found" "/block/$NOT_FOUND_HEIGHT" 404
else
    test_endpoint "Block Not Found" "/block/999999" 404
fi
echo ""

echo "=== Transaction Endpoints ==="
test_endpoint "Transaction Lookup (Seed TX)" "/tx/$TX_ID"
test_json_endpoint "Transaction Detail" "/tx/$TX_ID" ".tx_id"
test_json_endpoint "Transaction - Block Height" "/tx/$TX_ID" ".included_in.block_height"
test_json_endpoint "Transaction - Fairness Present" "/tx/$TX_ID" ".fairness.game_result.vrf.vrf_output"

test_endpoint "Transaction Not Found" "/tx/999999999999" 404
test_endpoint "Invalid Transaction ID" "/tx/abc" 400
echo ""

echo "=== Metrics Endpoint ==="
echo -n "Testing Prometheus Metrics... "
metrics=$(curl -s "$API_URL/metrics")
if echo "$metrics" | grep -q "atomiq_http_requests_total"; then
    echo -e "$PASS (Contains metrics)"
else
    echo -e "$FAIL (No metrics found)"
fi
echo ""

echo "=== Performance Test ==="
echo -n "Testing 100 concurrent requests... "
start_time=$(date +%s%N)
for i in {1..100}; do
    curl -s "$API_URL/health" > /dev/null &
done
wait
end_time=$(date +%s%N)
duration=$((($end_time - $start_time) / 1000000))
echo -e "$PASS (Completed in ${duration}ms)"
echo ""

echo "=== Response Time Test ==="
if command -v bc >/dev/null 2>&1; then
    echo -n "Testing /status response time... "
    response_time=$(curl -s -o /dev/null -w "%{time_total}" "$API_URL/status")
    response_ms=$(echo "$response_time * 1000" | bc)
    echo -e "$PASS (${response_ms}ms)"
else
    echo "Skipping /status response time test (missing 'bc')"
fi
echo ""

echo "======================================"
echo "  Test Suite Complete!"
echo "======================================"
