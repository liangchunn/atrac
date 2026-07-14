# Codec parity baseline

The architecture refactor preserves encoded bytes, priming and flush scheduling,
fallback policy, and RIFF/ATRACX container bytes for every supported profile.
The checked-in `architecture_parity` integration tests use deterministic PCM
and compact FNV-1a output fingerprints to detect drift without storing large
encoded fixtures.

Run the complete baseline with:

```text
cargo test --workspace
```

The baseline covers all five ATRAC3 profiles and all fourteen ATRAC3plus
profiles. It includes both ATRAC3 encoder strategies, mono and stereo channel
modes, exact and partial final PCM blocks, streaming/buffered equality for
ATRAC3plus, schedule/flush boundaries, malformed input, and injected sink
failures. Unit tests next to each boundary retain the more detailed scheduling,
container, and error assertions.

Byte equality is the acceptance criterion for every production profile. No
profile currently uses perceptual-only parity.

Native-derived comments that cite `docs/11` through `docs/14`, `decompiled/*`,
`tests/*` oracle files, addresses, or trace artifacts refer to the external
reverse-engineering archive used to recover this implementation. Those sources
are intentionally not shipped in this compact Rust workspace. The comments are
traceability notes, not links to repository-local files. The production parity
tests and the final external SHA-1 manifests are the maintained verification
sources in this repository and release process.
