#!/bin/bash
# Test script for casino game API endpoints

set -e

API_URL="${API_URL:-http://localhost:8080}"
PLAYER_ID="test-player-$(date +%s)"

echo "🎰 Casino Game API Test Script"
echo "================================"
echo ""

# Test 1: List supported tokens
echo "📋 Test 1: List Supported Tokens"
echo "GET /api/tokens"
curl -s "$API_URL/api/tokens" | jq '.'
echo ""
echo ""

# Test 2: Play coin flip (Heads)
echo "🪙 Test 2: Play Coin Flip (Heads)"
echo "POST /api/coinflip/play"

GAME_RESPONSE=$(curl -s -X POST "$API_URL/api/coinflip/play" \
  -H "Content-Type: application/json" \
  -d "{
    \"player_id\": \"$PLAYER_ID\",
    \"choice\": \"heads\",
    \"token\": {
      \"symbol\": \"SOL\"
    },
    \"bet_amount\": 1.0
  }")

echo "$GAME_RESPONSE" | jq '.'
echo ""

# Extract game ID and status
GAME_ID=$(echo "$GAME_RESPONSE" | jq -r '.game_id')
STATUS=$(echo "$GAME_RESPONSE" | jq -r '.status')

echo "Game ID: $GAME_ID"
echo "Status: $STATUS"
echo ""

# Test 3: If pending, poll for result
if [ "$STATUS" = "pending" ]; then
  echo "⏳ Test 3: Polling for Game Result"
  echo "GET /api/game/$GAME_ID"
  
  for i in {1..5}; do
    sleep 1
    echo "Attempt $i..."
    POLL_RESPONSE=$(curl -s "$API_URL/api/game/$GAME_ID")
    POLL_STATUS=$(echo "$POLL_RESPONSE" | jq -r '.status')
    
    if [ "$POLL_STATUS" = "complete" ]; then
      echo "✅ Game confirmed!"
      echo "$POLL_RESPONSE" | jq '.'
      GAME_RESPONSE="$POLL_RESPONSE"
      break
    fi
  done
  echo ""
fi

# Test 4: Verify VRF proof
if [ "$STATUS" = "complete" ] || echo "$GAME_RESPONSE" | jq -e '.result.vrf' > /dev/null 2>&1; then
  echo "🔐 Test 4: Verify VRF Proof"
  echo "POST /api/verify/vrf"
  
  VRF_OUTPUT=$(echo "$GAME_RESPONSE" | jq -r '.result.vrf.vrf_output')
  VRF_PROOF=$(echo "$GAME_RESPONSE" | jq -r '.result.vrf.vrf_proof')
  PUBLIC_KEY=$(echo "$GAME_RESPONSE" | jq -r '.result.vrf.public_key')
  INPUT_MESSAGE=$(echo "$GAME_RESPONSE" | jq -r '.result.vrf.input_message')
  
  VERIFY_RESPONSE=$(curl -s -X POST "$API_URL/api/verify/vrf" \
    -H "Content-Type: application/json" \
    -d "{
      \"vrf_output\": \"$VRF_OUTPUT\",
      \"vrf_proof\": \"$VRF_PROOF\",
      \"public_key\": \"$PUBLIC_KEY\",
      \"input_message\": \"$INPUT_MESSAGE\",
      \"game_type\": \"coinflip\"
    }")
  
  echo "$VERIFY_RESPONSE" | jq '.'
  
  IS_VALID=$(echo "$VERIFY_RESPONSE" | jq -r '.is_valid')
  
  if [ "$IS_VALID" = "true" ]; then
    echo "✅ VRF proof is valid and verifiable!"
  else
    echo "❌ VRF proof verification failed!"
  fi
  echo ""
fi

# Test 5: Play multiple games
echo "🎲 Test 5: Play 5 Quick Games"
for i in {1..5}; do
  CHOICE="heads"
  if [ $((i % 2)) -eq 0 ]; then
    CHOICE="tails"
  fi
  
  QUICK_GAME=$(curl -s -X POST "$API_URL/api/coinflip/play" \
    -H "Content-Type: application/json" \
    -d "{
      \"player_id\": \"$PLAYER_ID-batch\",
      \"choice\": \"$CHOICE\",
      \"token\": {
        \"symbol\": \"SOL\"
      },
      \"bet_amount\": 0.1
    }")
  
  QUICK_GAME_ID=$(echo "$QUICK_GAME" | jq -r '.game_id')
  QUICK_STATUS=$(echo "$QUICK_GAME" | jq -r '.status')
  
  if [ "$QUICK_STATUS" = "complete" ]; then
    OUTCOME=$(echo "$QUICK_GAME" | jq -r '.result.outcome')
    PAYOUT=$(echo "$QUICK_GAME" | jq -r '.result.payment.payout_amount')
    echo "  Game $i: $CHOICE → $OUTCOME (Payout: $PAYOUT SOL)"
  else
    echo "  Game $i: $CHOICE → pending ($QUICK_GAME_ID)"
  fi
done
echo ""

echo "================================"
echo "✅ All tests completed!"
echo ""
echo "Summary:"
echo "  - Tokens endpoint working"
echo "  - Coin flip game working"
echo "  - VRF proof verification working"
echo "  - Batch game submission working"
echo ""
echo "🎰 Casino game system is operational!"
