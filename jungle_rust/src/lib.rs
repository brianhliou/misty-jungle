//! PyO3 bindings for the vanilla Jungle engine.
//!
//! All board/movegen/search logic lives in `engine` (a pure, pyo3-free module shared with
//! the standalone `jungle-engine` UCI binary via a `#[path]` include). This file is only the
//! Python surface: thin `#[pyfunction]` forwarders the bakeoff + golden-parity harnesses call.

use pyo3::prelude::*;

mod engine;

/// Best move for a FEN, as engine UCI ("a1b1") or "(none)".
#[pyfunction]
fn best_move_from_fen(fen: String, node_budget: u64, time_ms: u64) -> String {
    match engine::state_from_fen(&fen) {
        Some(p) => {
            let m = engine::best_move(&p, node_budget, time_ms);
            if m.0 == 255 {
                "(none)".to_string()
            } else {
                engine::move_to_uci(m)
            }
        }
        None => "(none)".to_string(),
    }
}

/// Handcrafted-eval best move WITH repetition awareness + draw contempt. `rep_fens` = prior game
/// positions (since the last capture); `contempt` = how much the side to move dislikes a draw
/// (so an ahead/equal side avoids repetition draws and plays on).
#[pyfunction]
#[pyo3(signature = (fen, nodes, time_ms, rep_fens=Vec::new(), contempt=0))]
fn best_move_rep_from_fen(fen: String, nodes: u64, time_ms: u64, rep_fens: Vec<String>, contempt: i32) -> String {
    match engine::state_from_fen(&fen) {
        Some(p) => {
            let m = engine::best_move_scored_full(&p, None, None, nodes, time_ms, &rep_fens, contempt).0;
            if m.0 == 255 {
                "(none)".to_string()
            } else {
                engine::move_to_uci(m)
            }
        }
        None => "(none)".to_string(),
    }
}

/// Like `best_move_rep_from_fen` but with `ext` toggling the Rung-1 search extensions
/// (LMR + killer/history ordering + PVS). `ext=False` = pre-Rung-1 baseline, for A/B.
#[pyfunction]
#[pyo3(signature = (fen, nodes, time_ms, rep_fens=Vec::new(), contempt=0, ext=true))]
fn best_move_ext_from_fen(fen: String, nodes: u64, time_ms: u64, rep_fens: Vec<String>, contempt: i32, ext: bool) -> String {
    match engine::state_from_fen(&fen) {
        Some(p) => {
            let m = engine::best_move_scored_ext(&p, None, None, nodes, time_ms, &rep_fens, contempt, ext).0;
            if m.0 == 255 {
                "(none)".to_string()
            } else {
                engine::move_to_uci(m)
            }
        }
        None => "(none)".to_string(),
    }
}

/// TS-bot-equivalent reference opponent: fixed-depth αβ, no TT/ID/quiescence. For the
/// P1f bakeoff (full engine vs this isolates the search improvements).
#[pyfunction]
fn best_move_fixed_depth_from_fen(fen: String, depth: i32) -> String {
    match engine::state_from_fen(&fen) {
        Some(p) => {
            let m = engine::best_move_fixed_depth(&p, depth);
            if m.0 == 255 {
                "(none)".to_string()
            } else {
                engine::move_to_uci(m)
            }
        }
        None => "(none)".to_string(),
    }
}

/// Sorted legal moves (UCI strings) for a FEN. Drives the P1d golden-parity test vs
/// variants-jungle.ts.
#[pyfunction]
fn legal_moves_from_fen(fen: String) -> Vec<String> {
    match engine::state_from_fen(&fen) {
        Some(p) => {
            let mut v: Vec<String> = engine::legal_moves(&p)
                .into_iter()
                .map(engine::move_to_uci)
                .collect();
            v.sort();
            v
        }
        None => Vec::new(),
    }
}

/// Canonical starting FEN — sanity hook for the Python harness.
#[pyfunction]
fn initial_fen() -> String {
    engine::to_fen(&engine::initial())
}

fn status_str(s: engine::Status) -> String {
    match s {
        engine::Status::Playing => "playing",
        engine::Status::Win(0) => "red",
        engine::Status::Win(_) => "black",
        engine::Status::Draw => "draw",
    }
    .to_string()
}

/// Static status of a FEN: "playing" | "red" | "black" | "draw". For the golden test.
#[pyfunction]
fn status_from_fen(fen: String) -> String {
    match engine::state_from_fen(&fen) {
        Some(p) => status_str(engine::status_of(&p)),
        None => "invalid".to_string(),
    }
}

/// Root score (side-to-move perspective) from a deep search — to validate the tablebase.
#[pyfunction]
fn search_value_from_fen(fen: String, nodes: u64, time_ms: u64) -> i32 {
    match engine::state_from_fen(&fen) {
        Some(p) => engine::best_move_scored(&p, nodes, time_ms).1,
        None => 0,
    }
}

/// Best move + root score (stm POV) in ONE search — for gensfen self-play labeling.
#[pyfunction]
fn best_move_and_value_from_fen(fen: String, nodes: u64, time_ms: u64) -> (String, i32) {
    match engine::state_from_fen(&fen) {
        Some(p) => {
            let (m, sc) = engine::best_move_scored(&p, nodes, time_ms);
            let mv = if m.0 == 255 {
                "(none)".to_string()
            } else {
                engine::move_to_uci(m)
            };
            (mv, sc)
        }
        None => ("(none)".to_string(), 0),
    }
}

/// Solve a (2-piece) endgame; return (valid positions, wins, losses, draws).
#[pyfunction]
fn tb_solve_stats(pieces: Vec<u8>) -> (usize, usize, usize, usize) {
    engine::solve_endgame_stats(&pieces)
}

/// Solve the endgame and probe one FEN → "win" | "loss" | "draw" | "invalid" (stm perspective).
#[pyfunction]
fn tb_probe(pieces: Vec<u8>, fen: String) -> String {
    match engine::state_from_fen(&fen) {
        Some(p) => match engine::solve_and_probe(&pieces, &p) {
            engine::TB_WIN => "win",
            engine::TB_LOSS => "loss",
            _ => "draw",
        }
        .to_string(),
        None => "invalid".to_string(),
    }
}

/// Handcrafted eval (stm POV) for many positions at once — for R3 residual-target computation.
/// `squares_flat` = n×63 byte codes (0 empty, 1..8 red, 9..16 black), `stms` = n side-to-move
/// bytes (0 red, 1 black). Returns n eval_hand values (same units as the search score).
#[pyfunction]
fn eval_hand_batch(squares_flat: Vec<u8>, stms: Vec<u8>) -> Vec<i32> {
    stms.iter()
        .enumerate()
        .map(|(i, &t)| engine::eval_hand(&engine::parsed_from_squares(&squares_flat[i * 63..i * 63 + 63], t)))
        .collect()
}

/// Apply a UCI move to a FEN → (resulting FEN, resulting status). For the golden test.
#[pyfunction]
fn move_result_from_fen(fen: String, uci: String) -> (String, String) {
    match (engine::state_from_fen(&fen), engine::uci_to_move(&uci)) {
        (Some(p), Some(m)) => {
            let np = engine::make_move(&p, m);
            (engine::to_fen(&np), status_str(engine::board_status(&np, m.1)))
        }
        _ => ("invalid".to_string(), "invalid".to_string()),
    }
}

/// A built layered tablebase (2..=max_pieces). Build once, probe many.
#[pyclass]
struct JungleTb {
    store: std::collections::HashMap<Vec<u8>, Vec<u8>>,
}

#[pymethods]
impl JungleTb {
    #[new]
    fn new(max_pieces: usize) -> Self {
        JungleTb {
            store: engine::build_tables(max_pieces),
        }
    }

    /// WDL of a FEN from the side-to-move view: "win" | "loss" | "draw" | "nottb" | "invalid".
    fn probe(&self, fen: String) -> String {
        match engine::state_from_fen(&fen) {
            Some(p) => match engine::probe_store(&self.store, &p) {
                Some(engine::TB_WIN) => "win",
                Some(engine::TB_LOSS) => "loss",
                Some(_) => "draw",
                None => "nottb",
            }
            .to_string(),
            None => "invalid".to_string(),
        }
    }

    /// Per-piece-count aggregates: list of (k, sets, valid, win, loss, draw).
    fn stats(&self) -> Vec<(usize, usize, usize, usize, usize, usize)> {
        engine::store_stats(&self.store)
    }

    /// Solve `pieces` (using the built store for capture sub-tables) and sample up to `n`
    /// PLAYING positions with exact WDL: list of (squares[63] bytes, stm, wdl). The solved
    /// table is discarded on return. Requires the store to hold the (k-1) sub-tables.
    fn gen_labels(&self, pieces: Vec<u8>, n: usize, seed: u64) -> Vec<(Vec<u8>, u8, u8)> {
        engine::sample_set_labels(&pieces, &self.store, n, seed)
    }

    /// Serialize the full built store to disk (for a live tablebase loaded at engine startup).
    fn save(&self, path: String) -> PyResult<()> {
        engine::save_store(&self.store, &path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))
    }
}

/// The engine with an optional live tablebase (probed at the search leaf for exact endgames)
/// and/or a learned net eval. Either may be None.
#[pyclass]
struct JungleEngine {
    tb: Option<engine::Store>,
    net: Option<engine::Net>,
}

#[pymethods]
impl JungleEngine {
    #[new]
    #[pyo3(signature = (tb_path=None, net_path=None))]
    fn new(tb_path: Option<String>, net_path: Option<String>) -> PyResult<Self> {
        let tb = match tb_path {
            Some(p) => Some(engine::load_store(&p).ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!("could not load tb: {p}"))
            })?),
            None => None,
        };
        let net = match net_path {
            Some(p) => Some(engine::Net::load(&p).ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!("could not load net: {p}"))
            })?),
            None => None,
        };
        Ok(JungleEngine { tb, net })
    }

    /// `rep_fens` = prior game positions (since the last capture) for repetition detection;
    /// `contempt` = how much the side to move dislikes a draw (avoids draws when ahead/equal).
    #[pyo3(signature = (fen, nodes, time_ms, rep_fens=Vec::new(), contempt=0))]
    fn best_move(&self, fen: String, nodes: u64, time_ms: u64, rep_fens: Vec<String>, contempt: i32) -> String {
        match engine::state_from_fen(&fen) {
            Some(p) => {
                let m = engine::best_move_scored_full(
                    &p,
                    self.net.as_ref(),
                    self.tb.as_ref(),
                    nodes,
                    time_ms,
                    &rep_fens,
                    contempt,
                )
                .0;
                if m.0 == 255 {
                    "(none)".to_string()
                } else {
                    engine::move_to_uci(m)
                }
            }
            None => "(none)".to_string(),
        }
    }
}

/// All k-piece material sets (≥1 red, ≥1 black), codes ascending — for label-gen orchestration.
#[pyfunction]
fn tb_material_sets(k: usize) -> Vec<Vec<u8>> {
    engine::material_sets(k)
}

/// The engine with a learned (NNUE-style) leaf eval loaded from exported weights.
#[pyclass]
struct JungleNetEngine {
    net: engine::Net,
}

#[pymethods]
impl JungleNetEngine {
    #[new]
    fn new(weights_path: String) -> PyResult<Self> {
        engine::Net::load(&weights_path)
            .map(|net| JungleNetEngine { net })
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!("could not load net: {weights_path}"))
            })
    }

    /// Best move using the learned eval. Returns UCI or "(none)".
    fn best_move(&self, fen: String, nodes: u64, time_ms: u64) -> String {
        match engine::state_from_fen(&fen) {
            Some(p) => {
                let m = engine::best_move_scored_full(&p, Some(&self.net), None, nodes, time_ms, &[], 0).0;
                if m.0 == 255 {
                    "(none)".to_string()
                } else {
                    engine::move_to_uci(m)
                }
            }
            None => "(none)".to_string(),
        }
    }

    /// Raw learned-eval score (stm POV) for a FEN — for timing/inspection.
    fn eval(&self, fen: String) -> i32 {
        match engine::state_from_fen(&fen) {
            Some(p) => engine::eval_net(&p, &self.net),
            None => 0,
        }
    }
}

#[pymodule]
fn jungle_rust(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<JungleTb>()?;
    m.add_class::<JungleNetEngine>()?;
    m.add_class::<JungleEngine>()?;
    m.add_function(wrap_pyfunction!(best_move_from_fen, m)?)?;
    m.add_function(wrap_pyfunction!(best_move_rep_from_fen, m)?)?;
    m.add_function(wrap_pyfunction!(best_move_ext_from_fen, m)?)?;
    m.add_function(wrap_pyfunction!(best_move_fixed_depth_from_fen, m)?)?;
    m.add_function(wrap_pyfunction!(legal_moves_from_fen, m)?)?;
    m.add_function(wrap_pyfunction!(initial_fen, m)?)?;
    m.add_function(wrap_pyfunction!(status_from_fen, m)?)?;
    m.add_function(wrap_pyfunction!(move_result_from_fen, m)?)?;
    m.add_function(wrap_pyfunction!(eval_hand_batch, m)?)?;
    m.add_function(wrap_pyfunction!(search_value_from_fen, m)?)?;
    m.add_function(wrap_pyfunction!(best_move_and_value_from_fen, m)?)?;
    m.add_function(wrap_pyfunction!(tb_solve_stats, m)?)?;
    m.add_function(wrap_pyfunction!(tb_probe, m)?)?;
    m.add_function(wrap_pyfunction!(tb_material_sets, m)?)?;
    Ok(())
}
