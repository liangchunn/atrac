# ATRAC WebAssembly PoC

This static site loads the `at3` and `at3p` Rust encoders through a dedicated
WebAssembly bindings crate. It uses browser-native ES modules and a module Web
Worker; no JavaScript package manager or bundler is required.

## Build

Install the Rust target and `wasm-pack` once:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
```

From the repository root, build the release WebAssembly package:

```sh
cd crates/wasm
wasm-pack build --target web --release --out-dir ../../web/pkg --out-name atrac_wasm
```

Serve the static directory from the repository root:

```sh
python3 -m http.server 8000 --directory web
```

Then open <http://127.0.0.1:8000/>. Browsers cannot reliably initialize the
generated Wasm module when `index.html` is opened directly with a `file://` URL.

For automated local browser testing, the app accepts a `fixture` query only on
`localhost` and `127.0.0.1`. Serve a directory containing both the checkout and
fixture, then pass the fixture as a same-origin absolute path; the normal UI,
Worker, progress, checksum, and download flow are used unchanged.

## Streaming and performance

The JavaScript API exposes separate streaming classes for ATRAC3 and
ATRAC3plus. Each class accepts exactly the PCM frame count reported by
`expectedNextChunkFrames()`, reports codec progress, and returns the completed
RIFF/WAVE ATRAC file from `finish()`.

For the 127-second test WAV, this means about 5,470 4 KiB calls for ATRAC3 or
2,735 8 KiB calls for ATRAC3plus. Those copies cover the input bytes once and
are small compared with codec work. Encoding runs in a Worker so ATRAC3plus
does not block painting or interaction. The PoC buffers the input WAV and final
output; slice-based file reads and incremental output storage are deferred until
files large enough to justify their added complexity need to be supported.
