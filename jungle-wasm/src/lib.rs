//! MistyJungle WebAssembly shim — the in-browser client engine for Mistboard's vanilla
//! Jungle (Dou Shou Qi) review/analysis panel. Unlike the UCI binary (`jungle-engine`),
//! which emits a single `bestmove` for PvE play, this build exposes the search core's
//! **per-root-move exact values** (`root_move_values`) so the browser panel can render live
//! MultiPV: the top-K legal moves, each with its own eval, single-shot.
//!
//! Vanilla Jungle is PERFECT-INFORMATION (no face-down tiles, no pool), so there is no
//! redaction contract: the caller feeds a full-board Jungle FEN and the engine sees exactly
//! what the player sees. The FEN parser (`engine::state_from_fen`) is the SAME one the UCI
//! binary uses, so client and server build byte-identical states.
//!
//! Time source: `wasm32-unknown-unknown` has no monotonic clock (`Instant::now()` panics),
//! so `engine.rs` cfg-substitutes a no-op `Instant` on wasm and search is driven by the NODE
//! BUDGET only (`time_ms = 0`; `tick()` guards its wall-clock branch on `time_ms > 0`). Node
//! budget makes the search deterministic per position anyway.

#[path = "../../jungle_rust/src/engine.rs"]
#[allow(dead_code)] // engine.rs also exposes PyO3/UCI-facing entry points, unused here
mod engine;

use wasm_bindgen::prelude::*;

/// Evaluate a full-board Jungle FEN and return the top-`multipv` legal moves as JSON,
/// ranked best-first, each with an exact side-to-move centipawn score.
///
/// Returns `{"lines":[{"uci":"d1d2","cp":123,"depth":6},...]}`, or `{"error":"bad_fen"}` on a
/// malformed FEN, or `{"lines":[]}` when there is no legal move (terminal). `cp` is the
/// engine's native side-to-move score (WIN = 1_000_000; the browser normalizes POV to Red and
/// maps through its win% curve — a decisive |cp| renders as checkmate). The search
/// self-bounds its iterative deepening at the engine core's MAX_DEPTH.
#[wasm_bindgen]
pub fn analyze(fen: &str, nodes: u32, multipv: u32) -> String {
    let parsed = match engine::state_from_fen(fen) {
        Some(p) => p,
        None => return "{\"error\":\"bad_fen\"}".to_string(),
    };
    // ranked: Vec<(from, to, score, depth_reached)>, already sorted descending by score.
    let ranked = engine::root_move_values(&parsed, nodes as u64, 0);
    let take = (multipv.max(1) as usize).min(ranked.len());
    let mut out = String::from("{\"lines\":[");
    for (i, &(from, to, score, depth)) in ranked.iter().take(take).enumerate() {
        if i > 0 {
            out.push(',');
        }
        let uci = engine::move_to_uci((from, to));
        out.push_str(&format!("{{\"uci\":\"{uci}\",\"cp\":{score},\"depth\":{depth}}}"));
    }
    out.push_str("]}");
    out
}
