# MistyJungle

[![ci](https://github.com/brianhliou/misty-jungle/actions/workflows/ci.yml/badge.svg)](https://github.com/brianhliou/misty-jungle/actions/workflows/ci.yml)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A [Dou Shou Qi](https://en.wikipedia.org/wiki/Jungle_(board_game)) (Jungle, or Animal Chess) engine in Rust:
classical αβ search with a transposition table, iterative deepening, quiescence, and a handcrafted
evaluation. No neural network. It ships as a tiny UCI binary; the same search core is exposed to Python via PyO3.

Dou Shou Qi is a chase game on a 7×9 board. Each side has eight animals, ranked rat, cat, dog, wolf,
leopard, tiger, lion, elephant. A higher animal captures a lower one, with one twist that drives the
whole game: the rat captures the elephant. Two rivers split the board and only the rat may swim them;
the lion and tiger leap a river lengthwise. You win by stepping into the enemy den or capturing every
enemy piece. Material matters enormously, but a single rat can hold off an elephant, and a piece near
the enemy den is often worth more than anything it could capture.

The full build story is on my blog:
[The strongest open Dou Shou Qi engine I could build](https://brianhliou.com/posts/strong-dou-shou-qi-engine/).

## Strength (honest)

A strong αβ engine, measured rather than asserted:

- **Open field:** even to slightly ahead of the strongest open engine I could find and run, over 200 games,
  with zero rule disagreements between the two independent rule implementations.
- **Endgame:** provably near-perfect. Across 800 four-piece positions checked against an exact retrograde
  tablebase, the search never chose a move that turned a win into a loss; at five and six pieces it agreed
  with an eight-times-deeper search on better than 99% of positions.
- **Search polish:** late-move reductions, killer/history ordering, and principal-variation search were
  worth about +49 Elo (paired self-play, sign-tested on decisive games).

What it does **not** have is a human rating, and it won't until a serious human match earns one. I also
built endgame tablebases and a residual neural-network evaluation; at this engine's strength neither beat
the handcrafted version, which is most of what the blog post is about.

## Build

The UCI binary needs only a Rust toolchain (no Python):

```bash
cargo build -p jungle-engine --release
./target/release/jungle-engine
```

It speaks a minimal UCI-style protocol: `position`, then `go [movetime <ms>] [nodes <n>]`, and replies
`bestmove <uci>`. Strength is a node budget, which makes results reproducible across machines.

The Python bindings (`jungle_rust`, a PyO3 extension) build with [maturin](https://github.com/PyO3/maturin):

```bash
maturin develop --release   # inside a venv, from jungle_rust/
```

## Layout

- `jungle_rust/` — the engine core (`src/engine.rs`) plus PyO3 bindings (`src/lib.rs`).
- `jungle-engine/` — the standalone UCI binary, which `#[path]`-includes the engine core directly.

## License

MIT, see [LICENSE](LICENSE).
