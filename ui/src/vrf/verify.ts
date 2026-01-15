import { cryptoWaitReady, sr25519Verify } from "@polkadot/util-crypto";
import { hexToU8a, stringToU8a, u8aToHex } from "@polkadot/util";
import type { CoinChoice, TransactionResponse, VRFBundle } from "../api/types";

export type VerifyStatus =
  | { ok: true; derivedOutputHex: string; derivedCoin: CoinChoice }
  | {
      ok: false;
      reason: string;
      derivedOutputHex?: string;
      derivedCoin?: CoinChoice;
    };

function computeCoinflip(output: Uint8Array): CoinChoice {
  const firstByte = output[0] ?? 0;
  return firstByte % 2 === 0 ? "heads" : "tails";
}

async function sha256Digest(bytes: Uint8Array): Promise<Uint8Array> {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) {
    throw new Error("WebCrypto is not available in this environment");
  }
  const digest = await subtle.digest(
    "SHA-256",
    bytes as unknown as BufferSource
  );
  return new Uint8Array(digest);
}

export function buildExpectedInputMessage(params: {
  txId: number;
  gameType: string;
  playerAddress: string;
  blockHashHex: string;
  blockHeight: number;
  timestamp: number;
}): string {
  const context = `block_hash:${params.blockHashHex},tx:${params.txId},height:${params.blockHeight},time:${params.timestamp}`;
  return `tx-${params.txId}:${params.gameType}:${params.playerAddress}:${context}`;
}

export async function verifyVrfBundle(
  vrf: VRFBundle,
  expectedInputMessage?: string
): Promise<VerifyStatus> {
  await cryptoWaitReady();

  const input = expectedInputMessage ?? vrf.input_message;
  if (vrf.input_message !== input) {
    return {
      ok: false,
      reason: "VRF input_message mismatch vs expected input",
    };
  }

  let signature: Uint8Array;
  let publicKey: Uint8Array;
  let claimedOutput: Uint8Array;

  try {
    signature = hexToU8a(vrf.vrf_proof);
  } catch {
    return { ok: false, reason: "Invalid vrf_proof hex" };
  }

  try {
    publicKey = hexToU8a(vrf.public_key);
  } catch {
    return { ok: false, reason: "Invalid public_key hex" };
  }

  try {
    claimedOutput = hexToU8a(vrf.vrf_output);
  } catch {
    return { ok: false, reason: "Invalid vrf_output hex" };
  }

  if (signature.length !== 64) {
    return {
      ok: false,
      reason: `Invalid vrf_proof length (expected 64 bytes, got ${signature.length})`,
    };
  }
  if (publicKey.length !== 32) {
    return {
      ok: false,
      reason: `Invalid public_key length (expected 32 bytes, got ${publicKey.length})`,
    };
  }
  if (claimedOutput.length !== 32) {
    return {
      ok: false,
      reason: `Invalid vrf_output length (expected 32 bytes, got ${claimedOutput.length})`,
    };
  }

  const messageBytes = stringToU8a(input);
  const sigOk = sr25519Verify(messageBytes, signature, publicKey);
  if (!sigOk) {
    return { ok: false, reason: "sr25519 signature verification failed" };
  }

  const derivedOutput = await sha256Digest(signature);
  const derivedOutputHex = u8aToHex(derivedOutput).slice(2);
  const claimedOutputHex = u8aToHex(claimedOutput).slice(2);

  if (derivedOutputHex.toLowerCase() !== claimedOutputHex.toLowerCase()) {
    return {
      ok: false,
      reason: "vrf_output does not match sha256(vrf_proof)",
      derivedOutputHex,
      derivedCoin: computeCoinflip(derivedOutput),
    };
  }

  return {
    ok: true,
    derivedOutputHex,
    derivedCoin: computeCoinflip(derivedOutput),
  };
}

export async function verifyTxFairness(
  tx: TransactionResponse,
  pinnedPublicKeyHex?: string
): Promise<VerifyStatus> {
  const game = tx.fairness?.game_result;
  if (!game) {
    return { ok: false, reason: "No fairness.game_result attached to this tx" };
  }

  if (
    tx.included_in.block_height !== game.block_height ||
    tx.included_in.block_hash.toLowerCase() !== game.block_hash.toLowerCase()
  ) {
    return {
      ok: false,
      reason:
        "Inclusion mismatch: tx.included_in does not match game_result block",
    };
  }

  if (
    pinnedPublicKeyHex &&
    game.vrf.public_key.toLowerCase() !== pinnedPublicKeyHex.toLowerCase()
  ) {
    return {
      ok: false,
      reason: "VRF public key does not match pinned key (UI config)",
    };
  }

  const expectedInput = buildExpectedInputMessage({
    txId: game.transaction_id,
    gameType: game.game_type,
    playerAddress: game.player_address,
    blockHashHex: game.block_hash,
    blockHeight: game.block_height,
    timestamp: game.timestamp,
  });

  const vrfStatus = await verifyVrfBundle(game.vrf, expectedInput);
  if (!vrfStatus.ok) return vrfStatus;

  const expectedCoin = vrfStatus.derivedCoin;
  if (expectedCoin !== game.coin_result) {
    return {
      ok: false,
      reason: `Coin result mismatch: derived=${expectedCoin} stored=${game.coin_result}`,
      derivedOutputHex: vrfStatus.derivedOutputHex,
      derivedCoin: vrfStatus.derivedCoin,
    };
  }

  return vrfStatus;
}
