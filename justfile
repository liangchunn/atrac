default: wasm

# Build the browser-ready WebAssembly package without a JavaScript bundler.
wasm:
    cd crates/wasm && wasm-pack build --target web --release --out-dir ../../web/pkg --out-name atrac_wasm
