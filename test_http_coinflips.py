#!/usr/bin/env python3
"""
HTTP Coinflip Performance Test
==============================

This script tests the HTTP API coinflip endpoints and measures response times.
It performs 20 HTTP coinflip requests and calculates average response times
for transactions finalized with VRF.
"""

import asyncio
import aiohttp
import json
import time
import statistics
import uuid
from typing import List, Dict, Any

# Test configuration
API_BASE_URL = "http://localhost:8080"
NUM_TESTS = 20

async def test_health_check(session: aiohttp.ClientSession) -> bool:
    """Test if API server is responding"""
    try:
        async with session.get(f"{API_BASE_URL}/health") as response:
            if response.status == 200:
                data = await response.json()
                print(f"✅ API Health Check: {data.get('status', 'Unknown')}")
                return True
            else:
                print(f"❌ API Health Check failed: Status {response.status}")
                return False
    except Exception as e:
        print(f"❌ API Health Check error: {e}")
        return False

async def test_supported_tokens(session: aiohttp.ClientSession) -> List[str]:
    """Get list of supported tokens"""
    try:
        async with session.get(f"{API_BASE_URL}/api/tokens") as response:
            if response.status == 200:
                data = await response.json()
                tokens = [token["symbol"] for token in data.get("tokens", [])]
                print(f"📊 Supported tokens: {', '.join(tokens)}")
                return tokens
            else:
                print(f"⚠️  Could not fetch tokens (Status {response.status})")
                return ["SOL"]  # Fallback
    except Exception as e:
        print(f"⚠️  Error fetching tokens: {e}")
        return ["SOL"]  # Fallback

async def play_coinflip(session: aiohttp.ClientSession, choice: str, test_num: int) -> Dict[str, Any]:
    """Play a single coinflip game and measure response time"""
    
    # Create test data
    request_data = {
        "player_id": f"test-player-{test_num}",
        "choice": choice,
        "bet_amount": 1.0,
        "token": {"symbol": "SOL"}
    }
    
    start_time = time.time()
    
    try:
        async with session.post(
            f"{API_BASE_URL}/api/coinflip/play",
            json=request_data,
            headers={"Content-Type": "application/json"}
        ) as response:
            
            end_time = time.time()
            response_time_ms = (end_time - start_time) * 1000
            
            if response.status == 200:
                data = await response.json()
                
                # Extract game results
                game_id = data.get("game_id")
                result = data.get("result", {})
                outcome = result.get("outcome")
                vrf_proof = result.get("vrf_proof")
                
                return {
                    "success": True,
                    "test_num": test_num,
                    "response_time_ms": response_time_ms,
                    "game_id": game_id,
                    "choice": choice,
                    "outcome": outcome,
                    "vrf_verified": vrf_proof is not None,
                    "raw_response": data
                }
            else:
                text = await response.text()
                return {
                    "success": False,
                    "test_num": test_num,
                    "response_time_ms": response_time_ms,
                    "error": f"HTTP {response.status}: {text}",
                    "choice": choice
                }
                
    except Exception as e:
        end_time = time.time()
        response_time_ms = (end_time - start_time) * 1000
        return {
            "success": False,
            "test_num": test_num,
            "response_time_ms": response_time_ms,
            "error": str(e),
            "choice": choice
        }

async def verify_vrf_proof(session: aiohttp.ClientSession, game_id: str) -> bool:
    """Verify VRF proof for a game"""
    try:
        async with session.get(f"{API_BASE_URL}/api/verify/game/{game_id}") as response:
            if response.status == 200:
                data = await response.json()
                return data.get("is_valid", False)
            return False
    except:
        return False

async def main():
    """Main test execution"""
    
    print("🎰 HTTP Coinflip Performance Test")
    print("=" * 50)
    print(f"📊 Testing {NUM_TESTS} HTTP coinflip requests")
    print(f"🌐 API Base URL: {API_BASE_URL}")
    print()
    
    async with aiohttp.ClientSession() as session:
        
        # 1. Health check
        if not await test_health_check(session):
            print("❌ Cannot proceed - API server not responding")
            return
        
        print()
        
        # 2. Get supported tokens
        tokens = await test_supported_tokens(session)
        print()
        
        # 3. Perform coinflip tests
        print(f"🚀 Starting {NUM_TESTS} coinflip tests...")
        print()
        
        results = []
        choices = ["Heads", "Tails"]
        
        # Run tests concurrently (but with some rate limiting)
        semaphore = asyncio.Semaphore(5)  # Max 5 concurrent requests
        
        async def run_single_test(test_num: int):
            async with semaphore:
                choice = choices[test_num % 2]  # Alternate between Heads/Tails
                result = await play_coinflip(session, choice, test_num + 1)
                
                # Print real-time results
                if result["success"]:
                    print(f"✅ Test {result['test_num']:2d}: {choice:5s} → {result['outcome']:4s} | "
                          f"{result['response_time_ms']:6.1f}ms | VRF: {'✓' if result['vrf_verified'] else '✗'}")
                else:
                    print(f"❌ Test {result['test_num']:2d}: {choice:5s} → ERROR | "
                          f"{result['response_time_ms']:6.1f}ms | {result['error']}")
                
                return result
        
        # Execute all tests
        tasks = [run_single_test(i) for i in range(NUM_TESTS)]
        results = await asyncio.gather(*tasks)
        
        # 4. Analyze results
        print()
        print("📊 ANALYSIS")
        print("=" * 50)
        
        successful_results = [r for r in results if r["success"]]
        failed_results = [r for r in results if not r["success"]]
        
        print(f"✅ Successful requests: {len(successful_results)}/{NUM_TESTS}")
        print(f"❌ Failed requests: {len(failed_results)}/{NUM_TESTS}")
        
        if successful_results:
            response_times = [r["response_time_ms"] for r in successful_results]
            vrf_verified_count = sum(1 for r in successful_results if r.get("vrf_verified", False))
            
            print()
            print("⏱️  RESPONSE TIME ANALYSIS:")
            print(f"   Average: {statistics.mean(response_times):6.1f}ms")
            print(f"   Median:  {statistics.median(response_times):6.1f}ms")
            print(f"   Min:     {min(response_times):6.1f}ms")
            print(f"   Max:     {max(response_times):6.1f}ms")
            
            if len(response_times) > 1:
                print(f"   Std Dev: {statistics.stdev(response_times):6.1f}ms")
            
            print()
            print("🔐 VRF VERIFICATION:")
            print(f"   VRF Proofs Generated: {vrf_verified_count}/{len(successful_results)}")
            print(f"   VRF Success Rate:     {(vrf_verified_count/len(successful_results)*100):.1f}%")
            
            # Game outcome analysis
            outcomes = {}
            for r in successful_results:
                outcome = r.get("outcome", "Unknown")
                outcomes[outcome] = outcomes.get(outcome, 0) + 1
            
            print()
            print("🎯 GAME OUTCOMES:")
            for outcome, count in outcomes.items():
                percentage = (count / len(successful_results)) * 100
                print(f"   {outcome}: {count}/{len(successful_results)} ({percentage:.1f}%)")
        
        if failed_results:
            print()
            print("❌ FAILED REQUESTS:")
            for r in failed_results:
                print(f"   Test {r['test_num']}: {r['error']}")
        
        print()
        print("🎉 HTTP Coinflip Performance Test Complete!")

if __name__ == "__main__":
    asyncio.run(main())