//! MistyJungle — standalone vanilla Jungle (Dou Shou Qi) UCI engine, v0.0.6.
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
//!   go [movetime <ms>] [nodes <n>]   -> search, emit
//!                                       "info depth <d> nodes <n> nps <x> time <ms> score cp <n>
//!                                        pv <uci>" then "bestmove <uci>" (or "bestmove (none)"
//!                                       at a terminal)
//!   quit                             -> exit
//!
//! The `info … score cp` line is what the platform's whole-game analysis reads (the move
//! alone drives PvE play; analysis needs the position's evaluation). Score is side-to-move
//! POV in the search's native units (WIN = 1_000_000); the analysis layer owns POV
//! normalization + the win% curve, so the raw score is emitted as-is.
//!
//! The `depth`/`nodes`/`nps`/`time` fields in front of it are what the search CONSUMED, and
//! they exist so the host's per-move decision artifact can compare consumption against the
//! budget it granted. Without them wall time is the only signal, and wall time cannot tell
//! "the search got slower" from "the host got slower" — the same binary on the same position
//! took 2,227 ms at loadavg 14.9 and 3,587 ms at loadavg 86.3. `nodes` is therefore the real
//! visited count, never the requested budget echoed back: a field that repeats its own input
//! looks like measurement and answers nothing. `depth` is the last iterative-deepening
//! iteration that COMPLETED, and is omitted (rather than reported as 0) when the search was
//! cut short before finishing one; `nps` is omitted when elapsed rounds to 0 ms. Omitting a
//! field we do not have beats inventing one.

#[path = "../../jungle_rust/src/engine.rs"]
#[allow(dead_code)] // engine.rs also exposes the PyO3-facing helpers, unused here
mod engine;

use std::io::{self, BufRead, Write};
use std::time::Instant;

const ENGINE_NAME: &str = "MistyJungle 0.0.7";
const DEFAULT_MOVETIME_MS: u64 = 1000;
const DEFAULT_NODES: u64 = 1_000_000;
// A draw is worth -DRAW_CONTEMPT to the side to move, so a higher value makes an ahead-or-equal
// side decline a repetition and play on. Raised from 1 (effectively indifferent) on measurement:
// at a 5M-node budget over 100 self-play games per setting, contempt 1 gave 32% decisive games,
// 30 gave 34%, and 80 gave 50%. A self-play sweep alone proves nothing there, since both sides
// declining repetitions produces more decisive games by construction, so the two values were
// also faced off directly over 200 colour-swapped games: contempt 80 scored W30-L14-D156. It
// wins twice as often as it loses, so the decisiveness is not bought with worse play.
const DRAW_CONTEMPT: i32 = 80;

fn parse_go(line: &str) -> Result<(u64, u64, engine::SearchSkill), &'static str> {
    let mut nodes = DEFAULT_NODES;
    let mut movetime = DEFAULT_MOVETIME_MS;
    let mut skill = engine::SearchSkill::default();
    let mut words = line.split_whitespace().skip(1);
    while let Some(k) = words.next() {
        match k {
            "nodes" => {
                nodes = words
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|v| *v > 0)
                    .ok_or("nodes")?
            }
            "movetime" => {
                movetime = words
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("movetime")?
            }
            "depth" => {
                skill.max_depth = words
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|v| (1..=24).contains(v))
                    .ok_or("depth")?
            }
            "noise" => {
                skill.noise = words
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && (0.0..=100_000.0).contains(v))
                    .ok_or("noise")?
            }
            "seed" => skill.seed = words.next().and_then(|v| v.parse().ok()).ok_or("seed")?,
            _ => {}
        }
    }
    Ok((nodes, movetime, skill))
}

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

/// Build the standard UCI search-info line, in the standard field order, so a generic parser
/// picks it up. Fields the search did not actually produce are omitted, never estimated.
fn search_info_line(stats: &engine::SearchStats, elapsed_ms: u64, score: i32, pv: &str) -> String {
    let mut line = String::from("info");
    if stats.depth > 0 {
        line.push_str(&format!(" depth {}", stats.depth));
    }
    line.push_str(&format!(" nodes {}", stats.nodes));
    if elapsed_ms > 0 {
        line.push_str(&format!(
            " nps {}",
            stats.nodes.saturating_mul(1000) / elapsed_ms
        ));
    }
    line.push_str(&format!(" time {elapsed_ms} score cp {score} pv {pv}"));
    line
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
                // The no-capture clock is a RULE the host also enforces, and contempt shapes
                // every decision. An engine running different values than its host is invisible
                // until games diverge, so state both at handshake: they land in the host's engine
                // boot log, where a mismatch is one grep rather than a mystery.
                println!("info string progress_limit {}", engine::PROGRESS_LIMIT);
                println!("info string draw_contempt {DRAW_CONTEMPT}");
                println!("info string classical_skill 1");
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
                let (nodes, movetime, skill) = match parse_go(line) {
                    Ok(limits) => limits,
                    Err(field) => {
                        println!("info string invalid go {field}");
                        println!("bestmove (none)");
                        io::stdout().flush().ok();
                        continue;
                    }
                };
                match &current {
                    Some(p) => {
                        let started = Instant::now();
                        let (m, score, stats) = engine::best_move_scored_skill(
                            p,
                            None,
                            None,
                            nodes,
                            movetime,
                            &rep_fens,
                            DRAW_CONTEMPT,
                            true,
                            skill,
                        );
                        let elapsed_ms = started.elapsed().as_millis() as u64;
                        if m.0 == 255 {
                            println!("bestmove (none)");
                        } else {
                            let uci = engine::move_to_uci(m);
                            // Side-to-move POV score; the analysis layer normalizes POV and
                            // maps it through the win% curve (large win/loss magnitudes clamp).
                            println!("{}", search_info_line(&stats, elapsed_ms, score, &uci));
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

    #[test]
    fn skill_limits_fail_closed_and_defaults_preserve_the_top_search() {
        let (nodes, time, skill) = parse_go("go").unwrap();
        assert_eq!(
            (nodes, time, skill.max_depth, skill.noise),
            (DEFAULT_NODES, DEFAULT_MOVETIME_MS, 24, 0.0)
        );
        for bad in [
            "go noise NaN",
            "go noise -1",
            "go seed -1",
            "go depth 0",
            "go depth 25",
            "go nodes 0",
            "go noise",
        ] {
            assert!(parse_go(bad).is_err(), "{bad}");
        }
        let (_, _, skill) = parse_go("go depth 2 noise 350 seed 42").unwrap();
        assert_eq!((skill.max_depth, skill.noise, skill.seed), (2, 350.0, 42));
    }

    #[test]
    fn seeded_noise_is_reproducible_varied_legal_and_depth_bounded() {
        let (p, _) =
            parse_position_command("position fen l5t/1d3c1/r1p1w1e/7/7/7/E1W1P1R/1C3D1/T5L r 0 1")
                .unwrap();
        let run = |seed| {
            engine::best_move_scored_skill(
                &p,
                None,
                None,
                20_000,
                0,
                &[],
                DRAW_CONTEMPT,
                true,
                engine::SearchSkill {
                    max_depth: 1,
                    noise: 1000.0,
                    seed,
                },
            )
        };
        let a = run(42);
        let b = run(42);
        assert_eq!(
            (a.0, a.1, a.2.nodes, a.2.depth),
            (b.0, b.1, b.2.nodes, b.2.depth)
        );
        let mut choices = std::collections::HashSet::new();
        for seed in 0..32 {
            let (m, _, stats) = run(seed);
            assert!(engine::legal_moves(&p).contains(&m));
            assert_eq!(stats.depth, 1);
            choices.insert(m);
        }
        assert!(choices.len() > 4);
    }

    const CURRENT: &str = "7/3p3/1Tcedl1/3w3/1R1L3/4r2/4WE1/1t2C2/2D4 b 11 45";
    const REPEATED: &str = "7/3p3/1Tcedl1/3w3/1R1L3/4r2/4WE1/2t1C2/2D4 r 4 42";

    #[test]
    fn default_search_matches_frozen_shipping_006_vectors() {
        // Produced by the shipping 5717f7f tree, not by a second wrapper around
        // this implementation. Time is disabled; move/score/depth/nodes are exact.
        let vectors = [
            (
                "t5l/1cwp2d/e5r/7/7/R6/2P1WE1/LD4C/6T r 8 5",
                "a2a3",
                21,
                9,
                200001,
            ),
            (
                "tc5/5dl/e1w2pr/7/7/7/RP4E/5WC/LD4T r 10 6",
                "a1a2",
                -9,
                9,
                200003,
            ),
            (
                "t5l/1c1p3/ew3dr/7/7/3P3/R3WCE/D6/L5T r 12 7",
                "a1b1",
                6,
                8,
                200003,
            ),
        ];
        for (fen, best, cp, depth, nodes) in vectors {
            let p = engine::state_from_fen(fen).unwrap();
            let (m, score, stats) = engine::best_move_scored_stats(
                &p,
                None,
                None,
                200_000,
                0,
                &[],
                DRAW_CONTEMPT,
                true,
            );
            assert_eq!(
                (engine::move_to_uci(m), score, stats.depth, stats.nodes),
                (best.to_owned(), cp, depth, nodes),
                "{fen}"
            );
        }
    }

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
    fn search_info_reports_real_consumption_not_the_budget() {
        let (state, reps) =
            parse_position_command(&format!("position fen {CURRENT} reps {REPEATED}")).unwrap();
        let budget = 200_000;
        let (best, score, stats) = engine::best_move_scored_stats(
            &state,
            None,
            None,
            budget,
            60_000,
            &reps,
            DRAW_CONTEMPT,
            true,
        );
        // The node count is the search's own, so it lands near the cap without being it.
        assert!(stats.nodes > budget / 2, "{} nodes", stats.nodes);
        assert!(stats.nodes <= budget + 1, "{} nodes", stats.nodes);
        assert!(stats.depth >= 1);
        let line = search_info_line(&stats, 123, score, &engine::move_to_uci(best));
        assert!(
            line.starts_with(&format!(
                "info depth {} nodes {} nps ",
                stats.depth, stats.nodes
            )),
            "{line}"
        );
        assert!(
            line.contains(&format!(" time 123 score cp {score} pv ")),
            "{line}"
        );
    }

    #[test]
    fn search_info_omits_fields_the_search_did_not_produce() {
        // Nothing completed and no measurable elapsed: depth and nps are absent, not zeroed
        // or estimated. `nodes`, `time`, `score` and `pv` are always present.
        let stats = engine::SearchStats { nodes: 7, depth: 0 };
        assert_eq!(
            search_info_line(&stats, 0, -12, "a1b1"),
            "info nodes 7 time 0 score cp -12 pv a1b1"
        );
    }

    #[test]
    fn reported_game_avoids_the_immediate_threefold_draw() {
        let (state, reps) =
            parse_position_command(&format!("position fen {CURRENT} reps {REPEATED}")).unwrap();
        let (best, _) =
            engine::best_move_scored_full(&state, None, None, 500_000, 4_000, &reps, DRAW_CONTEMPT);
        assert_ne!(engine::move_to_uci(best), "b2c2");
    }
}
