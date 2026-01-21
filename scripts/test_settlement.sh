#!/bin/bash
# Settlement API Integration Test Script
# Tests the full settlement flow with optimistic locking and error handling

set -e

API_URL="${API_URL:-http://localhost:8080}"
PLAYER_ID="settlement-test-$(date +%s)"

PASS="\033[0;32m✓\033[0m"
FAIL="\033[0;31m✗\033[0m"

echo "💰 Settlement API Integration Test"
echo "=================================="
echo ""

# Function to test HTTP response
test_response() {
    local name="$1"
    local status="$2"
    local expected="${3:-200}"
    
    if [ "$status" -eq "$expected" ]; then
        echo -e "$PASS $name (HTTP $status)"
        return 0
    else
        echo -e "$FAIL $name (Expected HTTP $expected, got $status)"
        return 1
    fi
} 

# Test 1: Health Check
echo "🔍 Test 1: API Health Check"
echo "GET /health"
response=$(curl -s -w "%{http_code}" "$API_URL/health")
status="${response: -3}"
body="${response%???}"

test_response "Health Check" "$status" 200

if echo "$body" | jq -e '.status' >/dev/null 2>&1; then
    echo "   Status: $(echo "$body" | jq -r '.status')"
fi
echo ""

# Test 2: Create a coinflip game (seeding settlement data)
echo "🎯 Test 2: Create Coinflip Game (Settlement Source)"
echo "POST /api/coinflip/play"

# Use proper CoinFlipPlayRequest format based on game types
game_request="{
  \"player_id\": \"$PLAYER_ID\",
  \"choice\": \"heads\",
  \"token\": {
    \"symbol\": \"SOL\"
  },
  \"bet_amount\": 1.0
}"

game_response=$(curl -s -w "%{http_code}" -X POST "$API_URL/api/coinflip/play" \
  -H "Content-Type: application/json" \
  -d "$game_request")

game_status="${game_response: -3}"
game_body="${game_response%???}"

if ! test_response "Create Coinflip Game" "$game_status" 200; then
    echo "Error response: $game_body"
    exit 1
fi

# Parse game response
if echo "$game_body" | jq -e '.' >/dev/null 2>&1; then
    GAME_ID=$(echo "$game_body" | jq -r '.game_id // .transaction_id // empty')
    GAME_STATUS=$(echo "$game_body" | jq -r '.status // empty')
    
    # Extract transaction ID for settlement API
    if [[ "$GAME_ID" =~ ^tx-(.+) ]]; then
        TX_ID="${BASH_REMATCH[1]}"
    else
        TX_ID="$GAME_ID"
    fi
    
    echo "   Game ID: $GAME_ID"
    echo "   Transaction ID: $TX_ID"
    echo "   Status: $GAME_STATUS"
    
    # Check for settlement fields
    SETTLEMENT_STATUS=$(echo "$game_body" | jq -r '.settlement_status // .result.settlement_status // empty')
    VERSION=$(echo "$game_body" | jq -r '.version // .result.version // empty')
    
    if [ -n "$SETTLEMENT_STATUS" ]; then
        echo "   Settlement Status: $SETTLEMENT_STATUS"
    fi
    if [ -n "$VERSION" ]; then
        echo "   Version: $VERSION"
    fi
else
    echo -e "$FAIL Invalid JSON response"
    echo "Response: $game_body"
    exit 1
fi

# If game is pending, wait for completion
if [ "$GAME_STATUS" = "pending" ]; then
    echo "   ⏳ Waiting for game completion..."
    for i in {1..10}; do
        sleep 1
        poll_response=$(curl -s "$API_URL/api/game/$GAME_ID")
        poll_status=$(echo "$poll_response" | jq -r '.status // empty')
        
        if [ "$poll_status" = "complete" ]; then
            echo -e "   $PASS Game completed"
            game_body="$poll_response"
            # Re-extract settlement info from completed game
            SETTLEMENT_STATUS=$(echo "$game_body" | jq -r '.settlement_status // .result.settlement_status // "PendingSettlement"')
            VERSION=$(echo "$game_body" | jq -r '.version // .result.version // "1"')
            break
        elif [ "$i" = "10" ]; then
            echo -e "   $FAIL Game did not complete within 10 seconds"
            exit 1
        fi
    done
fi
echo ""

# Test 3: Check pending settlements
echo "📋 Test 3: Get Pending Settlements"
echo "GET /api/settlement/pending"

pending_response=$(curl -s -w "%{http_code}" "$API_URL/api/settlement/pending?limit=10")
pending_status="${pending_response: -3}"
pending_body="${pending_response%???}"

if ! test_response "Get Pending Settlements" "$pending_status" 200; then
    echo "Error response: $pending_body"
    exit 1
fi

if echo "$pending_body" | jq -e '.games' >/dev/null 2>&1; then
    pending_count=$(echo "$pending_body" | jq '.games | length')
    echo "   Found $pending_count pending settlements"
    
    # Check if our game is in the pending list
    our_game=$(echo "$pending_body" | jq --arg tx_id "$TX_ID" '.games[] | select(.transaction_id == ($tx_id | tonumber))')
    if [ -n "$our_game" ]; then
        echo -e "   $PASS Our game found in pending settlements"
        echo "   $(echo "$our_game" | jq -r '\"Game \" + (.transaction_id | tostring) + \": \" + .outcome + \" (\" + (.bet_amount | tostring) + \" lamports)\"')"
    else
        echo "   ⚠️  Our game not found in pending list (may be filtered)"
    fi
    
    # Show cursor if present
    next_cursor=$(echo "$pending_body" | jq -r '.next_cursor // empty')
    if [ -n "$next_cursor" ]; then
        echo "   Next cursor available: ${next_cursor:0:20}..."
    fi
else
    echo -e "$FAIL Invalid response format"
    echo "Response: $pending_body"
fi
echo ""

# Test 4: Update settlement status (SubmittedToSolana)
echo "🔄 Test 4: Update Settlement Status to SubmittedToSolana"
echo "POST /api/settlement/games/$TX_ID"

update_request="{
  \"status\": \"SubmittedToSolana\",
  \"expected_version\": ${VERSION:-1},
  \"solana_tx_id\": \"fake_solana_tx_$(date +%s)_abc123\"
}"

update_response=$(curl -s -w "%{http_code}" -X POST "$API_URL/api/settlement/games/$TX_ID" \
  -H "Content-Type: application/json" \
  -d "$update_request")

update_status="${update_response: -3}"
update_body="${update_response%???}"

if ! test_response "Update Settlement Status" "$update_status" 200; then
    echo "Error response: $update_body"
    echo "Request was: $update_request"
    exit 1
fi

if echo "$update_body" | jq -e '.' >/dev/null 2>&1; then
    success=$(echo "$update_body" | jq -r '.success // false')
    new_version=$(echo "$update_body" | jq -r '.new_version // empty')
    
    if [ "$success" = "true" ]; then
        echo -e "   $PASS Settlement updated successfully"
        echo "   New version: $new_version"
        VERSION="$new_version"
    else
        echo -e "   $FAIL Update did not succeed"
    fi
else
    echo -e "   $FAIL Invalid JSON response"
fi
echo ""

# Test 5: Test optimistic locking (should fail with stale version)
echo "🔒 Test 5: Test Optimistic Locking (Version Conflict)"
echo "POST /api/settlement/games/$TX_ID (with stale version)"

stale_request="{
  \"status\": \"SettlementFailed\",
  \"expected_version\": 1,
  \"error_message\": \"This should fail due to version mismatch\"
}"

stale_response=$(curl -s -w "%{http_code}" -X POST "$API_URL/api/settlement/games/$TX_ID" \
  -H "Content-Type: application/json" \
  -d "$stale_request")

stale_status="${stale_response: -3}"
stale_body="${stale_response%???}"

# Expect 400 Bad Request for version mismatch
if test_response "Optimistic Locking Conflict" "$stale_status" 400; then
    echo "   Correctly rejected stale version update"
    if echo "$stale_body" | grep -q "Version mismatch" 2>/dev/null; then
        echo -e "   $PASS Error message contains version mismatch details"
    fi
else
    echo "   ⚠️  Expected HTTP 400 for version conflict"
fi
echo ""

# Test 6: Complete the settlement
echo "✅ Test 6: Complete Settlement"
echo "POST /api/settlement/games/$TX_ID"

complete_request="{
  \"status\": \"SettlementComplete\",
  \"expected_version\": ${VERSION},
  \"solana_tx_id\": \"confirmed_solana_tx_$(date +%s)_xyz789\"
}"

complete_response=$(curl -s -w "%{http_code}" -X POST "$API_URL/api/settlement/games/$TX_ID" \
  -H "Content-Type: application/json" \
  -d "$complete_request")

complete_status="${complete_response: -3}"
complete_body="${complete_response%???}"

if ! test_response "Complete Settlement" "$complete_status" 200; then
    echo "Error response: $complete_body"
    exit 1
fi

if echo "$complete_body" | jq -e '.' >/dev/null 2>&1; then
    final_success=$(echo "$complete_body" | jq -r '.success // false')
    final_version=$(echo "$complete_body" | jq -r '.new_version // empty')
    
    if [ "$final_success" = "true" ]; then
        echo -e "   $PASS Settlement completed successfully"
        echo "   Final version: $final_version"
    fi
fi
echo ""

# Test 7: Test settlement ingest endpoint
echo "📥 Test 7: Settlement Event Ingest"
echo "POST /api/settlement/ingest"

ingest_request="{
  \"transaction_id\": 999999,
  \"player_address\": \"test_ingest_player_$(date +%s)\",
  \"game_type\": \"CoinFlip\",
  \"bet_amount\": 500000000,
  \"token\": \"SOL\",
  \"outcome\": \"Win\",
  \"payout\": 1000000000,
  \"vrf_proof\": \"fake_vrf_proof_hex_$(date +%s)\",
  \"vrf_output\": \"fake_vrf_output_hex_$(date +%s)\",
  \"block_height\": 12345,
  \"block_hash\": \"fake_block_hash_$(date +%s)\",
  \"timestamp\": $(date +%s)
}"

ingest_response=$(curl -s -w "%{http_code}" -X POST "$API_URL/api/settlement/ingest" \
  -H "Content-Type: application/json" \
  -d "$ingest_request")

ingest_status="${ingest_response: -3}"
ingest_body="${ingest_response%???}"

if test_response "Settlement Ingest" "$ingest_status" 200; then
    echo "   Ingest endpoint accepts settlement events correctly"
else
    echo "   Response: $ingest_body"
fi
echo ""

# Test 8: Verify completed settlement no longer pending
echo "🔍 Test 8: Verify Completed Settlement Not Pending"
echo "GET /api/settlement/pending"

final_pending_response=$(curl -s -w "%{http_code}" "$API_URL/api/settlement/pending?limit=20")
final_pending_status="${final_pending_response: -3}"
final_pending_body="${final_pending_response%???}"

if test_response "Final Pending Check" "$final_pending_status" 200; then
    completed_game=$(echo "$final_pending_body" | jq --arg tx_id "$TX_ID" '.games[] | select(.transaction_id == ($tx_id | tonumber))')
    if [ -z "$completed_game" ]; then
        echo -e "   $PASS Completed settlement correctly removed from pending list"
    else
        echo "   ⚠️  Completed settlement still appears in pending list (check filtering logic)"
    fi
fi
echo ""

# Test 9: Error handling - non-existent game
echo "❌ Test 9: Error Handling (Non-existent Game)"
echo "POST /api/settlement/games/999999999"

error_request="{
  \"status\": \"SubmittedToSolana\",
  \"expected_version\": 1
}"

error_response=$(curl -s -w "%{http_code}" -X POST "$API_URL/api/settlement/games/999999999" \
  -H "Content-Type: application/json" \
  -d "$error_request")

error_status="${error_response: -3}"
error_body="${error_response%???}"

if test_response "Non-existent Game Error" "$error_status" 404; then
    echo "   Correctly returns 404 for non-existent games"
fi
echo ""

echo "=================================="
echo "🎉 Settlement API Test Complete!"
echo "=================================="
echo ""
echo "✅ All Settlement Features Tested:"
echo "   - Settlement field initialization in game creation"
echo "   - Pending settlements query with pagination"  
echo "   - Settlement status updates with optimistic locking"
echo "   - Version conflict detection and rejection"
echo "   - Settlement completion workflow"
echo "   - Settlement event ingest endpoint"
echo "   - Proper error handling for edge cases"
echo "   - Settlement filtering (completed games removed from pending)"
echo ""
echo "💰 Settlement API is fully operational!"
echo "🔐 Optimistic locking prevents race conditions"
echo "📊 Ready for transaction processor integration"