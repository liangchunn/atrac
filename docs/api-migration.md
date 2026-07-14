# Encoder API migration

The supported encoding APIs now live at each crate root. Implementation
modules, native-layout adapters, DSP stages, coding passes, tables, and packer
leaves are private.

ATRAC3 callers should replace `at3::encoder::stream::Atrac3StreamConfig` with a
validated `at3::Atrac3Profile`, and use `at3::Atrac3Encoder`. Stream errors,
progress, summaries, phases, and write stages are available as `EncodeError`,
`EncodeProgress`, `EncodeSummary`, `EncodePhase`, and `WriteStage`.

ATRAC3plus callers should construct `at3p::Atrac3plusProfile` and use
`at3p::Atrac3plusEncoder`. Complete buffered input can use
`at3p::encode_to_vec`. RIFF/WAVE inspection helpers remain deliberately public
under `at3p::container`.

Both streaming encoders now expose `expected_next_chunk_frames`, a
`push_pcm` method that returns `Result<(), EncodeError>`, optional progress
callbacks, and a consuming `finish` that returns `(writer, summary)`. Their
summaries share `input_sample_frames`, `output_frames`, `payload_bytes`, and
`file_bytes`; ATRAC3 also reports encoded and fallback frame counts. See
[`encoder-usage.md`](encoder-usage.md) for the complete PCM and container
contract.

`at3p::encoder::api::PublicEncoderHandle` and the other native-compatible
surfaces have been removed. Their encoder methods were permanent stubs and are
not part of the supported crate-root API.
