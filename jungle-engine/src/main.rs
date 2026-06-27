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
//!   go [movetime <ms>] [nodes <n>]   -> search, emit "bestmove <uci>" (or "(none)")
//!   quit                             -> exit
//!
//! NOTE (scaffold): search is a stub returning "(none)" until P1c (movegen) + P1e (search) land.

#[path = "../../jungle_rust/src/engine.rs"]
#[allow(dead_code)] // engine.rs also exposes the PyO3-facing helpers, unused here
mod engine;

use std::io::{self, BufRead, Write};

const ENGINE_NAME: &str = "MistyJungle 0.0.1";
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
                let mv = match &current {
                    Some(p) => {
                        let m = engine::best_move(p, nodes, movetime);
                        if m.0 == 255 {
                            "(none)".to_string()
                        } else {
                            engine::move_to_uci(m)
                        }
                    }
                    None => "(none)".to_string(),
                };
                println!("bestmove {mv}");
            }
            "quit" => break,
            _ => {}
        }
        io::stdout().flush().ok();
    }
}
