# MistyJungle

[![ci](https://github.com/brianhliou/misty-jungle/actions/workflows/ci.yml/badge.svg)](https://github.com/brianhliou/misty-jungle/actions/workflows/ci.yml)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A [Dou Shou Qi](https://en.wikipedia.org/wiki/Jungle_(board_game)) (Jungle, or Animal Chess)
engine in Rust: alpha-beta search with a transposition table, iterative deepening, quiescence,
and a handcrafted evaluation. No neural network. It ships as a small UCI binary, with the same
search core exposed to Python via PyO3.

Dou Shou Qi is a 7×9 chase game. Eight animals rank from rat to elephant, and a higher animal
captures a lower one, with one twist: the rat captures the elephant. Rivers split the board and
only the rat may swim them; lions and tigers leap across. You win by reaching the enemy den or
capturing every piece.

## Strength

Even to slightly ahead of the strongest open Dou Shou Qi engine I could find and run, over a
200-game match. Its endgame play is near-perfect, verified against exact retrograde tablebases.

The full build story is on my blog:
[The strongest open Dou Shou Qi engine I could build](https://brianhliou.com/posts/strong-dou-shou-qi-engine/).

## Build

The UCI binary needs only a Rust toolchain:

```bash
cargo build -p jungle-engine --release
./target/release/jungle-engine
```

It speaks a minimal UCI-style protocol: `position`, then `go [movetime <ms>] [nodes <n>]`,
replying `bestmove <uci>`. Strength is a node budget, so results are reproducible across machines.

The Python bindings (`jungle_rust`) build with [maturin](https://github.com/PyO3/maturin):

```bash
maturin develop --release
```

## Layout

- `jungle_rust/`: the engine core (`src/engine.rs`) and the PyO3 bindings (`src/lib.rs`).
- `jungle-engine/`: the UCI binary, which `#[path]`-includes the engine core.

## License

MIT, see [LICENSE](LICENSE).
