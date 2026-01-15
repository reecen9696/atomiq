import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { apiTx } from "../api/client";
import type { TransactionResponse } from "../api/types";
import { verifyTxFairness } from "../vrf/verify";

export function VerifyPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [txId, setTxId] = useState(() => searchParams.get("tx") || "");
  const [loading, setLoading] = useState(false);
  const [tx, setTx] = useState<TransactionResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [verifyStatus, setVerifyStatus] = useState<{
    ok: boolean;
    text: string;
    details?: string;
  } | null>(null);

  const pinnedKey =
    (import.meta.env.VITE_PINNED_VRF_PUBLIC_KEY_HEX as string | undefined) ||
    undefined;

  useEffect(() => {
    const q = searchParams.get("tx");
    if (q && q !== txId) setTxId(q);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams]);

  const normalizedTxId = useMemo(() => txId.trim(), [txId]);
  const txVrfPubkey = tx?.fairness?.game_result?.vrf?.public_key;

  async function run() {
    if (!normalizedTxId) return;

    setLoading(true);
    setError(null);
    setVerifyStatus(null);
    setTx(null);

    try {
      const fetched = await apiTx(normalizedTxId);
      setTx(fetched);

      const vrf = await verifyTxFairness(fetched, pinnedKey);
      if (vrf.ok) {
        setVerifyStatus({
          ok: true,
          text: "Verified (sr25519 signature + sha256(output) + coinflip mapping)",
          details: `derivedCoin=${vrf.derivedCoin} derivedOutput=${vrf.derivedOutputHex}`,
        });
      } else {
        setVerifyStatus({
          ok: false,
          text: "Verification failed",
          details: vrf.reason,
        });
      }

      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        next.set("tx", normalizedTxId);
        return next;
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div style={{ display: "grid", gap: 14 }}>
      <div className="panel">
        <div style={{ fontWeight: 700 }}>Verify</div>
        <div className="muted">
          Fetches <span className="mono">/tx/:tx_id</span> and verifies the VRF
          proof locally in your browser.
        </div>

        <div style={{ height: 12 }} />

        <div className="row">
          <div className="field" style={{ minWidth: 360, flex: 1 }}>
            <label>Transaction ID</label>
            <input
              value={txId}
              onChange={(e) => setTxId(e.target.value)}
              placeholder="tx-123 or 123"
              className="mono"
            />
            <div className="muted">
              Accepts numeric IDs or <span className="mono">tx-</span> prefix.
            </div>
          </div>

          <div className="buttons" style={{ alignItems: "flex-end" }}>
            <button
              className="btn btnPrimary"
              onClick={() => run()}
              disabled={loading || !normalizedTxId}
            >
              {loading ? "Verifying…" : "Verify"}
            </button>
          </div>
        </div>

        {pinnedKey ? (
          <div className="muted" style={{ marginTop: 10 }}>
            Pinned VRF pubkey (UI): <span className="mono">{pinnedKey}</span>
          </div>
        ) : (
          <div className="muted" style={{ marginTop: 10 }}>
            No pinned VRF pubkey configured (optional). Verification still runs
            locally, using the VRF pubkey embedded in the transaction. Set{" "}
            <span className="mono">VITE_PINNED_VRF_PUBLIC_KEY_HEX</span> to
            hard-pin and fail verification if the server tries to rotate keys.
          </div>
        )}

        {!pinnedKey && txVrfPubkey ? (
          <div className="muted" style={{ marginTop: 10 }}>
            VRF pubkey used for this tx: <span className="mono">{txVrfPubkey}</span>
          </div>
        ) : null}

        {error ? (
          <div style={{ marginTop: 12 }} className="muted">
            Error: {error}
          </div>
        ) : null}

        {verifyStatus ? (
          <div style={{ marginTop: 12 }}>
            <div className={verifyStatus.ok ? "pill pillOk" : "pill pillBad"}>
              {verifyStatus.ok ? "ok" : "fail"}
            </div>
            <div style={{ height: 8 }} />
            <div>{verifyStatus.text}</div>
            {verifyStatus.details ? (
              <div className="muted mono">{verifyStatus.details}</div>
            ) : null}
          </div>
        ) : null}
      </div>

      {tx ? <TxDetails tx={tx} /> : null}
    </div>
  );
}

function TxDetails({ tx }: { tx: TransactionResponse }) {
  const game = tx.fairness?.game_result;

  return (
    <div className="panel" style={{ display: "grid", gap: 12 }}>
      <div style={{ fontWeight: 650 }}>Transaction</div>

      <div className="row">
        <div>
          <div className="muted">tx_id</div>
          <div className="mono">{tx.tx_id}</div>
        </div>
        <div>
          <div className="muted">tx_hash</div>
          <div className="mono" style={{ wordBreak: "break-all" }}>
            {tx.tx_hash}
          </div>
        </div>
      </div>

      <div className="row">
        <div>
          <div className="muted">Included in</div>
          <div className="mono">
            height={tx.included_in.block_height} index={tx.included_in.index}
          </div>
          <div className="mono" style={{ wordBreak: "break-all" }}>
            {tx.included_in.block_hash}
          </div>
        </div>
      </div>

      <div>
        <div className="muted">Fairness record</div>
        {game ? (
          <div style={{ display: "grid", gap: 10 }}>
            <div className="row">
              <div>
                <div className="muted">player</div>
                <div className="mono">{game.player_address}</div>
              </div>
              <div>
                <div className="muted">bet</div>
                <div className="mono">
                  {game.bet_amount} {game.token.symbol} ({game.player_choice})
                </div>
              </div>
              <div>
                <div className="muted">coin_result</div>
                <div className="mono">{game.coin_result}</div>
              </div>
              <div>
                <div className="muted">outcome</div>
                <div
                  className={
                    game.outcome === "win" ? "pill pillOk" : "pill pillBad"
                  }
                >
                  {game.outcome}
                </div>
              </div>
            </div>

            <div>
              <div className="muted">VRF input_message</div>
              <div className="mono" style={{ wordBreak: "break-all" }}>
                {game.vrf.input_message}
              </div>
            </div>

            <div className="row">
              <div>
                <div className="muted">vrf_output</div>
                <div className="mono" style={{ wordBreak: "break-all" }}>
                  {game.vrf.vrf_output}
                </div>
              </div>
              <div>
                <div className="muted">vrf_proof</div>
                <div className="mono" style={{ wordBreak: "break-all" }}>
                  {game.vrf.vrf_proof}
                </div>
              </div>
              <div>
                <div className="muted">public_key</div>
                <div className="mono" style={{ wordBreak: "break-all" }}>
                  {game.vrf.public_key}
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="muted">
            No game_result attached (tx may not be a game bet, or not persisted
            yet).
          </div>
        )}
      </div>
    </div>
  );
}
