//! Golden-output test for the geometry maths (ENG-04).
//!
//! The library is empty: everything lives in `tests/golden.rs`, which computes a fixed set of
//! `libm` functions, compositions of them and the exact predicates of `geom`, and compares the
//! result bit for bit with the committed `golden.txt`. CI runs it natively on Linux, macOS and
//! Windows and under `wasm32-wasip1` with wasmtime, so a target that gives other bits fails the
//! build. Rationale: `docs/dev/adr/0003-deterministic-math-library.md`.
