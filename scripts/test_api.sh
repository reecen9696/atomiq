#!/bin/bash
# Comprehensive API Test Script

set -e

API_URL="http://localhost:8080"
PASS="\033[0;32m✓\033[0m"
FAIL="\033[0;31m✗\033[0m"

echo "======================================"
echo "  Atomiq API Comprehensive Test Suite"
echo "======================================"
echo ""

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

echo "=== Block Endpoints ==="
test_endpoint "Block List" "/blocks"
test_json_endpoint "Block List - Pagination" "/blocks?limit=5" ".pagination.total_returned"
test_endpoint "Block Detail (Height 1)" "/block/1"
test_endpoint "Block Detail (Height 2)" "/block/2"
test_endpoint "Block Not Found" "/block/999" 404
echo ""

echo "=== Transaction Endpoints ==="
test_endpoint "Transaction Lookup (ID 1)" "/tx/1"
test_json_endpoint "Transaction Detail" "/tx/1" ".tx_id"
test_json_endpoint "Transaction - Block Height" "/tx/1" ".included_in.block_height"
test_endpoint "Transaction Not Found" "/tx/999999" 404
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
echo -n "Testing /status response time... "
response_time=$(curl -s -o /dev/null -w "%{time_total}" "$API_URL/status")
response_ms=$(echo "$response_time * 1000" | bc)
echo -e "$PASS (${response_ms}ms)"
echo ""

echo "======================================"
echo "  Test Suite Complete!"
echo "======================================"
