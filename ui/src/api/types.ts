export type CoinChoice = "heads" | "tails";

export interface Token {
  symbol: string;
  mint_address?: string | null;
}

export type GameType = "coinflip";

export type GameOutcome = "win" | "loss";
export type CoinFlipResult = "heads" | "tails";

export interface VRFBundle {
  vrf_output: string; // hex
  vrf_proof: string; // hex (64 bytes)
  public_key: string; // hex (32 bytes)
  input_message: string;
}

export interface CoinFlipPlayRequest {
  player_id: string;
  choice: CoinChoice;
  token: Token;
  bet_amount: number;
  wallet_signature?: string | null;
}

export interface GameResult {
  game_id: string;
  game_type: GameType;
  player: {
    player_id: string;
    wallet_signature?: string | null;
  };
  payment: {
    token: Token;
    bet_amount: number;
    payout_amount: number;
    settlement_tx_id?: string | null;
  };
  vrf: VRFBundle;
  outcome: GameOutcome;
  timestamp: number;
  game_type_data: "coinflip";
  player_choice: CoinChoice;
  result_choice: CoinChoice;
  block_height?: number;
  block_hash?: string;
  finalization_confirmed?: boolean;
}

export type GameResponse =
  | { status: "complete"; game_id: string; result: GameResult }
  | { status: "pending"; game_id: string; message?: string | null };

export interface StatusResponse {
  node_info: { id: string; network: string; version: string };
  sync_info: {
    latest_block_height: number;
    latest_block_hash: string;
    latest_block_time: string;
    catching_up: boolean;
  };
}

export interface RecentGameSummary {
  game_id: string;
  tx_id: number;
  player_id: string;
  game_type: GameType;
  token: Token;
  bet_amount: number;
  player_choice: CoinChoice;
  coin_result: CoinFlipResult;
  outcome: GameOutcome;
  payout: number;
  timestamp: number;
  block_height: number;
  block_hash: string;
}

export interface RecentGamesResponse {
  games: RecentGameSummary[];
  next_cursor?: string | null;
}

export interface TransactionResponse {
  tx_id: string;
  tx_hash: string;
  included_in: {
    block_height: number;
    block_hash: string;
    index: number;
  };
  type: string;
  data: {
    sender: string;
    data: string;
    timestamp: number;
    nonce: number;
  };
  fairness?: {
    game_bet?: {
      game_type: GameType;
      bet_amount: number;
      token: Token;
      player_choice: CoinChoice;
      player_address: string;
    } | null;
    game_result?: {
      transaction_id: number;
      player_address: string;
      game_type: GameType;
      bet_amount: number;
      token: Token;
      player_choice: CoinChoice;
      coin_result: CoinFlipResult;
      outcome: GameOutcome;
      vrf: VRFBundle;
      payout: number;
      timestamp: number;
      block_height: number;
      block_hash: string;
    } | null;
  } | null;
}
