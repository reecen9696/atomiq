import type {
  CoinFlipPlayRequest,
  GameResponse,
  RecentGamesResponse,
  StatusResponse,
  Token,
  TransactionResponse,
} from "./types";

async function fetchJson<T>(
  input: RequestInfo | URL,
  init?: RequestInit
): Promise<{ data: T; headers: Headers }> {
  const res = await fetch(input, {
    ...init,
    headers: {
      "content-type": "application/json",
      ...(init?.headers || {}),
    },
  });

  const text = await res.text();
  if (!res.ok) {
    throw new Error(text || `HTTP ${res.status}`);
  }

  const data = text ? (JSON.parse(text) as T) : (null as unknown as T);
  return { data, headers: res.headers };
}

export async function apiStatus(): Promise<StatusResponse> {
  const { data } = await fetchJson<StatusResponse>("/status");
  return data;
}

export async function apiTokens(): Promise<Token[]> {
  const { data } = await fetchJson<Token[]>("/api/tokens");
  return data;
}

export async function apiPlayCoinflip(req: CoinFlipPlayRequest): Promise<{
  response: GameResponse;
  responseTimeMs?: number;
  responseHeaderMs?: number;
  responseWallMs?: number;
}> {
  const start = performance.now();
  const { data, headers } = await fetchJson<GameResponse>(
    "/api/coinflip/play",
    {
      method: "POST",
      body: JSON.stringify(req),
    }
  );
  const end = performance.now();

  const header = headers.get("x-response-time-ms");
  const responseHeaderMs = header ? Number(header) : undefined;
  const responseWallMs = Math.round(end - start);
  const responseTimeMs = Number.isFinite(responseHeaderMs)
    ? responseHeaderMs
    : responseWallMs;

  return {
    response: data,
    responseTimeMs: Number.isFinite(responseTimeMs)
      ? responseTimeMs
      : undefined,
    responseHeaderMs: Number.isFinite(responseHeaderMs)
      ? responseHeaderMs
      : undefined,
    responseWallMs: Number.isFinite(responseWallMs)
      ? responseWallMs
      : undefined,
  };
}

export async function apiGameById(gameId: string): Promise<GameResponse> {
  const { data } = await fetchJson<GameResponse>(
    `/api/game/${encodeURIComponent(gameId)}`
  );
  return data;
}

export async function apiRecentGames(
  limit = 10,
  cursor?: string
): Promise<RecentGamesResponse> {
  const url = new URL("/api/games/recent", window.location.origin);
  url.searchParams.set("limit", String(limit));
  if (cursor) url.searchParams.set("cursor", cursor);

  const { data } = await fetchJson<RecentGamesResponse>(
    url.pathname + url.search
  );
  return data;
}

export async function apiTx(txId: string): Promise<TransactionResponse> {
  const normalized = txId.trim().startsWith("tx-")
    ? txId.trim().slice(3)
    : txId.trim();
  const { data } = await fetchJson<TransactionResponse>(
    `/tx/${encodeURIComponent(normalized)}`
  );
  return data;
}
