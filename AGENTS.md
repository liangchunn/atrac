# Repository Guidelines

## Project Structure & Module Organization

This Rust 2024 workspace is pinned to Rust 1.96 and contains four crates:

- `crates/at3`: ATRAC3 codec library, split into analysis, core coding, encoder, and tables.
- `crates/at3p`: ATRAC3plus library, organized into DSP, coding, entropy, typed pipeline, bitstream, encoder, and RIFF handling.
- `crates/cli`: the `atrac` command-line encoder built on both library façades.
- `crates/wasm`: WebAssembly bindings for the codec façades.

Unit tests live beside implementation code under `#[cfg(test)]`; integration and architecture tests are in `crates/*/tests/`. Design contracts live in `docs/`; `tools/` contains parity helpers, `docker/` packages the legacy oracle, and `web/` contains the browser demo. Do not commit `target/` or generated audio outputs.

## Build, Test, and Development Commands

- `cargo build --workspace`: compile every crate in development mode.
- `cargo build --release --locked`: create optimized, lockfile-reproducible binaries.
- `cargo run -p cli -- encode -b 192 input.wav output.wav`: run the encoder against 16-bit, 44.1 kHz mono/stereo PCM WAV.
- `cd crates/wasm && wasm-pack build --target web --release --out-dir ../../web/pkg --out-name atrac_wasm`: rebuild browser bindings after WASM API changes.
- `cargo test --workspace`: run unit, integration, parity, and architecture-boundary tests.
- `cargo fmt --all -- --check`: verify standard Rust formatting.
- `cargo clippy --workspace --all-targets -- -D clippy::large_enum_variant -D clippy::type_complexity`: enforce the architecture-focused lint gate.

## Coding Style & Naming Conventions

Use `rustfmt` defaults (four-space indentation and trailing commas). Follow Rust conventions: `snake_case` for modules, functions, and tests; `UpperCamelCase` for types and traits; `SCREAMING_SNAKE_CASE` for constants. Keep codec internals independent: share lifecycle vocabulary through crate-root façades, not implementation modules. Preserve the dependency direction documented in `docs/architecture.md`; coding must not import packing, and tables must not depend on higher layers.

## Testing Guidelines

Name tests after observable behavior, such as `payload_writer_failures_are_typed`. Add focused unit tests near changed logic and integration tests for public contracts. Encoding changes must preserve the exact byte fingerprints in `architecture_parity`; there is no perceptual-only acceptance path or numeric coverage target. Run the full workspace suite before submitting.

## Commit & Pull Request Guidelines

History follows Conventional Commit-style subjects: `fix(at3p): ...`, `refactor(cli): ...`, `test: ...`, and `docs: ...`. Use an imperative, concise subject with the narrowest useful scope. Pull requests should explain the behavior and architecture impact, list validation commands, and link relevant issues. Call out parity fingerprint changes explicitly and justify them; include CLI output examples when user-facing behavior changes. Keep unrelated cleanup in separate commits.
