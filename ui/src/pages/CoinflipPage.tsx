import { useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import {
  apiGameById,
  apiPlayCoinflip,
  apiRecentGames,
  apiStatus,
  apiTokens,
} from "../api/client";
import type {
  CoinChoice,
  GameResponse,
  RecentGameSummary,
  Token,
} from "../api/types";
import { BarChart } from "../components/BarChart";

const UNITS_PER_TOKEN = 1_000_000_000;

function formatUnits(units: number): string {
  const value = units / UNITS_PER_TOKEN;
  return value.toLocaleString(undefined, { maximumFractionDigits: 6 });
}

function nowId(): string {
  return `player-${Math.random().toString(16).slice(2, 10)}`;
}

type LoadSample = {
  i: number;
  when: number;
  choice: CoinChoice;
  ms?: number;
  serverMs?: number;
  wallMs?: number;
  ok: boolean;
  error?: string;
};

function percentile(values: number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const idx = Math.min(
    sorted.length - 1,
    Math.max(0, Math.ceil((p / 100) * sorted.length) - 1)
  );
  return sorted[idx]!;
}

export function CoinflipPage() {
  const [tokens, setTokens] = useState<Token[]>([]);
  const [statusHeight, setStatusHeight] = useState<number | null>(null);
  const [playerId, setPlayerId] = useState<string>(
    () => localStorage.getItem("player_id") || nowId()
  );
  const [token, setToken] = useState<Token>(() => ({ symbol: "SOL" }));
  const [betAmount, setBetAmount] = useState<number>(0.1);
  const [submitting, setSubmitting] = useState(false);
  const [lastResponse, setLastResponse] = useState<GameResponse | null>(null);
  const [lastResponseMs, setLastResponseMs] = useState<number | undefined>(
    undefined
  );
  const [recent, setRecent] = useState<RecentGameSummary[]>([]);
  const [recentError, setRecentError] = useState<string | null>(null);

  const [loadCount, setLoadCount] = useState<number>(20);
  const [loadConcurrency, setLoadConcurrency] = useState<number>(1);
  const [loadChoiceMode, setLoadChoiceMode] = useState<
    "alternate" | "random" | "heads" | "tails"
  >("alternate");
  const [loadRunning, setLoadRunning] = useState(false);
  const [loadSamples, setLoadSamples] = useState<LoadSample[]>([]);
  const loadAbortRef = useRef<{ abort: boolean } | null>(null);

  const pollAbortRef = useRef<{ abort: boolean } | null>(null);

  useEffect(() => {
    localStorage.setItem("player_id", playerId);
  }, [playerId]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [t, s] = await Promise.all([apiTokens(), apiStatus()]);
        if (cancelled) return;
        setTokens(t);
        if (t.length > 0) setToken(t[0]!);
        setStatusHeight(s.sync_info.latest_block_height);
      } catch {
        // Non-fatal; page still works if backend isn't up yet.
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  async function refreshStatusAndRecent() {
    try {
      const [s, r] = await Promise.all([apiStatus(), apiRecentGames(10)]);
      setStatusHeight(s.sync_info.latest_block_height);
      setRecent(r.games);
      setRecentError(null);
    } catch (e) {
      setRecentError(e instanceof Error ? e.message : String(e));
    }
  }

  useEffect(() => {
    refreshStatusAndRecent();
    const interval = window.setInterval(() => refreshStatusAndRecent(), 2500);
    return () => window.clearInterval(interval);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const normalizedTokenJson = useMemo(() => JSON.stringify(token), [token]);

  const loadOkTimes = useMemo(
    () =>
      loadSamples
        .filter((s) => s.ok && typeof s.ms === "number")
        .map((s) => s.ms!),
    [loadSamples]
  );
  const loadServerTimes = useMemo(
    () =>
      loadSamples
        .filter((s) => s.ok && typeof s.serverMs === "number")
        .map((s) => s.serverMs!),
    [loadSamples]
  );
  const loadWallTimes = useMemo(
    () =>
      loadSamples
        .filter((s) => s.ok && typeof s.wallMs === "number")
        .map((s) => s.wallMs!),
    [loadSamples]
  );

  const loadStats = useMemo(() => {
    const times = loadOkTimes;
    if (times.length === 0) {
      return {
        total: loadSamples.length,
        ok: loadSamples.filter((s) => s.ok).length,
        err: loadSamples.filter((s) => !s.ok).length,
        min: 0,
        max: 0,
        avg: 0,
        p50: 0,
        p95: 0,
      };
    }
    const sum = times.reduce((a, b) => a + b, 0);
    return {
      total: loadSamples.length,
      ok: loadSamples.filter((s) => s.ok).length,
      err: loadSamples.filter((s) => !s.ok).length,
      min: Math.min(...times),
      max: Math.max(...times),
      avg: Math.round(sum / times.length),
      p50: percentile(times, 50),
      p95: percentile(times, 95),
    };
  }, [loadOkTimes, loadSamples]);

  function pickChoice(i: number): CoinChoice {
    switch (loadChoiceMode) {
      case "heads":
        return "heads";
      case "tails":
        return "tails";
      case "random":
        return Math.random() < 0.5 ? "heads" : "tails";
      default:
        return i % 2 === 0 ? "heads" : "tails";
    }
  }

  async function runLoadTest() {
    if (loadRunning) return;

    const count = Math.max(1, Math.min(10_000, Math.floor(loadCount)));
    const concurrency = Math.max(1, Math.min(64, Math.floor(loadConcurrency)));

    // cancel any active run
    if (loadAbortRef.current) loadAbortRef.current.abort = true;
    loadAbortRef.current = { abort: false };
    const tokenRef = loadAbortRef.current;

    setLoadRunning(true);
    setLoadSamples([]);

    const worker = async (workerIndex: number) => {
      for (let i = workerIndex; i < count; i += concurrency) {
        if (!tokenRef || tokenRef.abort) return;
        const choice = pickChoice(i);

        try {
          const { responseTimeMs, responseHeaderMs, responseWallMs } =
            await apiPlayCoinflip({
              player_id: playerId,
              choice,
              token,
              bet_amount: betAmount,
            });

          setLoadSamples((prev) =>
            prev.concat({
              i,
              when: Date.now(),
              choice,
              ok: true,
              ms: responseTimeMs,
              serverMs: responseHeaderMs,
              wallMs: responseWallMs,
            })
          );
        } catch (e) {
          setLoadSamples((prev) =>
            prev.concat({
              i,
              when: Date.now(),
              choice,
              ok: false,
              error: e instanceof Error ? e.message : String(e),
            })
          );
        }
      }
    };

    try {
      await Promise.all(
        Array.from({ length: concurrency }, (_, idx) => worker(idx))
      );
      refreshStatusAndRecent();
    } finally {
      setLoadRunning(false);
    }
  }

  function cancelLoadTest() {
    if (loadAbortRef.current) loadAbortRef.current.abort = true;
    setLoadRunning(false);
  }

  async function play(choice: CoinChoice) {
    if (submitting) return;

    // cancel any active poll
    if (pollAbortRef.current) pollAbortRef.current.abort = true;
    pollAbortRef.current = { abort: false };

    setSubmitting(true);
    setLastResponse(null);
    setLastResponseMs(undefined);

    try {
      const { response, responseTimeMs } = await apiPlayCoinflip({
        player_id: playerId,
        choice,
        token,
        bet_amount: betAmount,
      });
      setLastResponse(response);
      setLastResponseMs(responseTimeMs);

      if (response.status === "pending") {
        const gameId = response.game_id;
        const pollToken = pollAbortRef.current;

        for (let attempt = 0; attempt < 20; attempt++) {
          if (!pollToken || pollToken.abort) break;
          await new Promise((r) => setTimeout(r, 600));
          const next = await apiGameById(gameId);
          setLastResponse(next);
          if (next.status === "complete") break;
        }
      }

      refreshStatusAndRecent();
    } catch (e) {
      setLastResponse({
        status: "pending",
        game_id: "n/a",
        message: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div style={{ display: "grid", gap: 14 }}>
      <div className="panel">
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            gap: 12,
            flexWrap: "wrap",
          }}
        >
          <div>
            <div style={{ fontWeight: 700 }}>Bet (Coinflip)</div>
            <div className="muted">
              Latest block height:{" "}
              <span className="mono">{statusHeight ?? "—"}</span>
            </div>
          </div>
          <div className="buttons">
            <button
              className="btn"
              onClick={() => refreshStatusAndRecent()}
              disabled={submitting}
            >
              Refresh
            </button>
          </div>
        </div>

        <div style={{ height: 12 }} />

        <div className="row">
          <div className="field">
            <label>Player ID</label>
            <input
              value={playerId}
              onChange={(e) => setPlayerId(e.target.value)}
              placeholder="player-..."
            />
          </div>

          <div className="field">
            <label>Token</label>
            <select
              value={normalizedTokenJson}
              onChange={(e) => setToken(JSON.parse(e.target.value) as Token)}
            >
              {(tokens.length ? tokens : [{ symbol: "SOL" }]).map((t) => (
                <option key={t.symbol} value={JSON.stringify(t)}>
                  {t.symbol}
                </option>
              ))}
            </select>
          </div>

          <div className="field">
            <label>Bet amount</label>
            <input
              type="number"
              step="0.0001"
              value={betAmount}
              onChange={(e) => setBetAmount(Number(e.target.value))}
            />
            <div className="muted">
              Backend converts to smallest unit internally.
            </div>
          </div>
        </div>

        <div style={{ height: 12 }} />

        <div className="buttons">
          <button
            className="btn btnPrimary"
            disabled={submitting}
            onClick={() => play("heads")}
          >
            Bet Heads
          </button>
          <button
            className="btn btnPrimary"
            disabled={submitting}
            onClick={() => play("tails")}
          >
            Bet Tails
          </button>
          {submitting ? <span className="muted">Submitting…</span> : null}
        </div>

        {lastResponse ? (
          <div style={{ marginTop: 14, display: "grid", gap: 10 }}>
            <div className="row">
              <div>
                <span className="muted">Status:</span>{" "}
                <span
                  className={
                    lastResponse.status === "complete" ? "pill pillOk" : "pill"
                  }
                >
                  {lastResponse.status}
                </span>
              </div>
              {lastResponseMs != null ? (
                <div>
                  <span className="muted">x-response-time-ms:</span>{" "}
                  <span className="mono">{lastResponseMs}</span>
                </div>
              ) : null}
            </div>

            <div>
              <div className="muted">Game ID</div>
              <div className="mono">{lastResponse.game_id}</div>
            </div>

            {lastResponse.status === "pending" ? (
              <div className="muted">{lastResponse.message || "Pending…"}</div>
            ) : (
              <div
                className="panel"
                style={{ background: "rgba(37, 99, 235, 0.03)" }}
              >
                <div
                  className="row"
                  style={{ justifyContent: "space-between" }}
                >
                  <div>
                    <div style={{ fontWeight: 700 }}>Result</div>
                    <div className="muted">
                      Outcome is committed in a finalized block.
                    </div>
                  </div>
                  <Link
                    className="btn"
                    to={`/verify?tx=${encodeURIComponent(
                      lastResponse.result.game_id
                    )}`}
                  >
                    Verify in browser
                  </Link>
                </div>

                <div style={{ height: 10 }} />

                <div className="row">
                  <div>
                    <div className="muted">Outcome</div>
                    <div
                      className={
                        lastResponse.result.outcome === "win"
                          ? "pill pillOk"
                          : "pill pillBad"
                      }
                    >
                      {lastResponse.result.outcome}
                    </div>
                  </div>
                  <div>
                    <div className="muted">Player choice</div>
                    <div className="mono">
                      {lastResponse.result.player_choice}
                    </div>
                  </div>
                  <div>
                    <div className="muted">Coin result</div>
                    <div className="mono">
                      {lastResponse.result.result_choice}
                    </div>
                  </div>
                  <div>
                    <div className="muted">Payout</div>
                    <div className="mono">
                      {lastResponse.result.payment.payout_amount}{" "}
                      {lastResponse.result.payment.token.symbol}
                    </div>
                  </div>
                </div>

                <div style={{ height: 10 }} />

                <div>
                  <div className="muted">VRF input</div>
                  <div className="mono" style={{ wordBreak: "break-all" }}>
                    {lastResponse.result.vrf.input_message}
                  </div>
                </div>
              </div>
            )}
          </div>
        ) : null}
      </div>

      <div className="panel">
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            gap: 12,
            flexWrap: "wrap",
          }}
        >
          <div>
            <div style={{ fontWeight: 700 }}>Load test</div>
            <div className="muted">
              Send N bets and chart response time (ms).
            </div>
          </div>
          <div className="buttons">
            <button
              className="btn"
              disabled={loadRunning}
              onClick={() => setLoadSamples([])}
            >
              Clear
            </button>
          </div>
        </div>

        <div style={{ height: 12 }} />

        <div className="row">
          <div className="field">
            <label>Bets</label>
            <input
              type="number"
              min={1}
              max={10000}
              step={1}
              value={loadCount}
              onChange={(e) => setLoadCount(Number(e.target.value))}
            />
          </div>

          <div className="field">
            <label>Concurrency</label>
            <input
              type="number"
              min={1}
              max={64}
              step={1}
              value={loadConcurrency}
              onChange={(e) => setLoadConcurrency(Number(e.target.value))}
            />
            <div className="muted">1 = sequential. Higher = parallel.</div>
          </div>

          <div className="field">
            <label>Choice mode</label>
            <select
              value={loadChoiceMode}
              onChange={(e) =>
                setLoadChoiceMode(e.target.value as typeof loadChoiceMode)
              }
            >
              <option value="alternate">Alternate</option>
              <option value="random">Random</option>
              <option value="heads">Always heads</option>
              <option value="tails">Always tails</option>
            </select>
          </div>
        </div>

        <div style={{ height: 12 }} />

        <div className="buttons">
          <button
            className="btn btnPrimary"
            disabled={loadRunning}
            onClick={() => runLoadTest()}
          >
            {loadRunning ? "Running…" : "Run"}
          </button>
          <button
            className="btn"
            disabled={!loadRunning}
            onClick={() => cancelLoadTest()}
          >
            Cancel
          </button>
          <div className="muted">
            total <span className="mono">{loadStats.total}</span> · ok{" "}
            <span className="mono">{loadStats.ok}</span> · err{" "}
            <span className="mono">{loadStats.err}</span>
          </div>
        </div>

        <div style={{ height: 12 }} />

        <div className="row" style={{ alignItems: "flex-start" }}>
          <div style={{ flex: 1 }}>
            <div className="muted" style={{ marginBottom: 6 }}>
              Response time series (ms)
            </div>
            <BarChart values={loadOkTimes} height={140} />
          </div>
          <div style={{ minWidth: 260 }}>
            <div className="muted" style={{ marginBottom: 6 }}>
              Stats (ms)
            </div>
            <div
              className="panel"
              style={{ background: "rgba(2, 6, 23, 0.03)" }}
            >
              <div className="row" style={{ justifyContent: "space-between" }}>
                <div className="muted">avg</div>
                <div className="mono">{loadStats.avg}</div>
              </div>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <div className="muted">p50</div>
                <div className="mono">{loadStats.p50}</div>
              </div>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <div className="muted">p95</div>
                <div className="mono">{loadStats.p95}</div>
              </div>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <div className="muted">min</div>
                <div className="mono">{loadStats.min}</div>
              </div>
              <div className="row" style={{ justifyContent: "space-between" }}>
                <div className="muted">max</div>
                <div className="mono">{loadStats.max}</div>
              </div>
              <div style={{ height: 8 }} />
              <div className="muted" style={{ fontSize: 12 }}>
                Timing source: <span className="mono">x-response-time-ms</span>{" "}
                when present; otherwise wall-clock.
              </div>
              {loadServerTimes.length > 0 || loadWallTimes.length > 0 ? (
                <div className="muted" style={{ fontSize: 12, marginTop: 6 }}>
                  captured: server{" "}
                  <span className="mono">{loadServerTimes.length}</span> · wall{" "}
                  <span className="mono">{loadWallTimes.length}</span>
                </div>
              ) : null}
            </div>
          </div>
        </div>

        {loadSamples.some((s) => !s.ok) ? (
          <div style={{ marginTop: 12 }}>
            <div className="muted" style={{ marginBottom: 6 }}>
              Errors
            </div>
            <div
              className="mono"
              style={{ fontSize: 12, whiteSpace: "pre-wrap" }}
            >
              {loadSamples
                .filter((s) => !s.ok)
                .slice(0, 5)
                .map((s) => `#${s.i}: ${s.error}`)
                .join("\n")}
              {loadSamples.filter((s) => !s.ok).length > 5 ? "\n…" : ""}
            </div>
          </div>
        ) : null}
      </div>

      <div className="panel">
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            gap: 12,
            flexWrap: "wrap",
          }}
        >
          <div>
            <div style={{ fontWeight: 650 }}>Recent games</div>
            <div className="muted">From /api/games/recent</div>
          </div>
          <div className="muted">
            {recentError ? `Error: ${recentError}` : null}
          </div>
        </div>

        <div style={{ height: 10 }} />

        <table className="table">
          <thead>
            <tr>
              <th>Tx</th>
              <th>Player</th>
              <th>Bet</th>
              <th>Result</th>
              <th>Outcome</th>
              <th>Block</th>
            </tr>
          </thead>
          <tbody>
            {recent.map((g) => (
              <tr key={g.game_id}>
                <td className="mono">
                  <Link to={`/verify?tx=${encodeURIComponent(g.game_id)}`}>
                    {g.game_id}
                  </Link>
                </td>
                <td className="mono">{g.player_id}</td>
                <td className="mono">
                  {formatUnits(g.bet_amount)} {g.token.symbol} (
                  {g.player_choice})
                </td>
                <td className="mono">{g.coin_result}</td>
                <td>
                  <span
                    className={
                      g.outcome === "win" ? "pill pillOk" : "pill pillBad"
                    }
                  >
                    {g.outcome}
                  </span>
                </td>
                <td className="mono">{g.block_height}</td>
              </tr>
            ))}
            {recent.length === 0 ? (
              <tr>
                <td colSpan={6} className="muted">
                  No games yet.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  );
}
