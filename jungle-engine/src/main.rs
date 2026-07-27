//! MistyJungle — standalone vanilla Jungle (Dou Shou Qi) UCI engine, v0.0.3.
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
//!   position fen <FEN> [reps <FEN>;...] [moves ...]
//!                                    -> store the position + twice-seen repetition seeds
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

const ENGINE_NAME: &str = "MistyJungle 0.0.3";
const DEFAULT_MOVETIME_MS: u64 = 1000;
const DEFAULT_NODES: u64 = 1_000_000;
const DRAW_CONTEMPT: i32 = 1;

fn parse_position_command(line: &str) -> Option<(engine::Parsed, Vec<String>)> {
    let rest = line.strip_prefix("position")?.trim();
    let fen_and_reps = rest.strip_prefix("fen")?.trim();
    // Preserve tolerance for standard UCI `moves` suffixes. Mistboard sends repetition
    // seeds instead, but offline callers may still append a replay list.
    let fen_and_reps = fen_and_reps
        .split_once(" moves ")
        .map_or(fen_and_reps, |(position, _)| position);
    let (fen, rep_fens) = match fen_and_reps.split_once(" reps ") {
        Some((fen, reps)) => (
            fen.trim(),
            reps.split(';')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_owned)
                .collect(),
        ),
        None => (fen_and_reps, Vec::new()),
    };
    engine::state_from_fen(fen).map(|state| (state, rep_fens))
}

fn main() {
    let stdin = io::stdin();
    let mut current: Option<engine::Parsed> = None;
    let mut rep_fens: Vec<String> = Vec::new();
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
            "ucinewgame" => {
                current = None;
                rep_fens.clear();
            }
            "position" => {
                if let Some((state, seeds)) = parse_position_command(line) {
                    current = Some(state);
                    rep_fens = seeds;
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
                        let (m, score) = engine::best_move_scored_full(
                            p,
                            None,
                            None,
                            nodes,
                            movetime,
                            &rep_fens,
                            DRAW_CONTEMPT,
                        );
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

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT: &str = "7/3p3/1Tcedl1/3w3/1R1L3/4r2/4WE1/1t2C2/2D4 b 11 45";
    const REPEATED: &str = "7/3p3/1Tcedl1/3w3/1R1L3/4r2/4WE1/2t1C2/2D4 r 4 42";

    #[test]
    fn position_command_parses_repetition_seed_fens() {
        let (state, reps) =
            parse_position_command(&format!("position fen {CURRENT} reps {REPEATED}")).unwrap();
        assert_eq!(engine::to_fen(&state), CURRENT);
        assert_eq!(reps, vec![REPEATED]);
    }

    #[test]
    fn position_command_keeps_accepting_a_moves_suffix() {
        let (state, reps) =
            parse_position_command(&format!("position fen {CURRENT} moves b2c2")).unwrap();
        assert_eq!(engine::to_fen(&state), CURRENT);
        assert!(reps.is_empty());
    }

    #[test]
    fn reported_game_avoids_the_immediate_threefold_draw() {
        let (state, reps) =
            parse_position_command(&format!("position fen {CURRENT} reps {REPEATED}")).unwrap();
        let (best, _) = engine::best_move_scored_full(
            &state,
            None,
            None,
            500_000,
            4_000,
            &reps,
            DRAW_CONTEMPT,
        );
        assert_ne!(engine::move_to_uci(best), "b2c2");
    }
}
