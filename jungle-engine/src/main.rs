//! MistyJungle — standalone vanilla Jungle (Dou Shou Qi) UCI engine, v0.0.1 (scaffold).
//!
//! Perfect-information, deterministic 7×9 game → plain negamax αβ (no chance nodes, no
//! redaction). Reuses the SAME board/movegen/search core as the PyO3 lib + Python bakeoffs
//! (`jungle_rust/src/engine.rs`, included via `#[path]`) behind a tiny UCI front-end, so the
//! mistboard platform spawns + drives it exactly like the banqi/jieqi Tier-B binaries.
//!
//! Protocol (subset of UCI):
//!   uci                              -> id name/author, uciok
//!   isready                          -> readyok
//!   ucinewgame                       -> clear position
//!   position fen <FEN> [moves ...]   -> store the position (perfect-info: full board, no redaction)
//!   go [movetime <ms>] [nodes <n>]   -> search, emit "info score cp <n> pv <uci>" then
//!                                       "bestmove <uci>" (or "bestmove (none)" at a terminal)
//!   quit                             -> exit
//!
//! The `info … score cp` line is what the platform's whole-game analysis reads (the move
//! alone drives PvE play; analysis needs the position's evaluation). Score is side-to-move
//! POV in the search's native units (WIN = 1_000_000); the analysis layer owns POV
//! normalization + the win% curve, so the raw score is emitted as-is.

#[path = "../../jungle_rust/src/engine.rs"]
#[allow(dead_code)] // engine.rs also exposes the PyO3-facing helpers, unused here
mod engine;

use std::io::{self, BufRead, Write};

const ENGINE_NAME: &str = "MistyJungle 0.0.2";
const DEFAULT_MOVETIME_MS: u64 = 1000;
const DEFAULT_NODES: u64 = 1_000_000;

fn main() {
    let stdin = io::stdin();
    let mut current: Option<engine::Parsed> = None;
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim();
        let cmd = line.split_whitespace().next().unwrap_or("");
        match cmd {
            "uci" => {
                println!("id name {ENGINE_NAME}");
                println!("id author Mistboard");
                println!("uciok");
            }
            "isready" => println!("readyok"),
            "ucinewgame" => current = None,
            "position" => {
                if let Some(rest) = line.strip_prefix("position") {
                    if let Some(fenpart) = rest.trim().strip_prefix("fen") {
                        let fenstr = fenpart.split(" moves ").next().unwrap_or(fenpart).trim();
                        current = engine::state_from_fen(fenstr);
                    }
                }
            }
            "go" => {
                let mut movetime = DEFAULT_MOVETIME_MS;
                let mut nodes = DEFAULT_NODES;
                let mut t = line.split_whitespace().skip(1);
                while let Some(k) = t.next() {
                    match k {
                        "movetime" => {
                            if let Some(v) = t.next() {
                                movetime = v.parse().unwrap_or(movetime);
                            }
                        }
                        "nodes" => {
                            if let Some(v) = t.next() {
                                nodes = v.parse().unwrap_or(nodes);
                            }
                        }
                        _ => {}
                    }
                }
                match &current {
                    Some(p) => {
                        let (m, score) = engine::best_move_scored(p, nodes, movetime);
                        if m.0 == 255 {
                            println!("bestmove (none)");
                        } else {
                            let uci = engine::move_to_uci(m);
                            // Side-to-move POV score; the analysis layer normalizes POV and
                            // maps it through the win% curve (large win/loss magnitudes clamp).
                            println!("info score cp {score} pv {uci}");
                            println!("bestmove {uci}");
                        }
                    }
                    None => println!("bestmove (none)"),
                }
            }
            "quit" => break,
            _ => {}
        }
        io::stdout().flush().ok();
    }
}
