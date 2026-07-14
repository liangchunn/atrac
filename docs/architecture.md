# Architecture

The workspace has two independent codec libraries and one CLI consumer. The
libraries intentionally share lifecycle vocabulary, not codec internals.

## Public boundary

`at3` and `at3p` expose validated profiles, streaming encoders, progress and
summary values, typed errors, and channel-layout types from their crate roots.
`at3p` additionally exposes deliberate RIFF/WAVE inspection helpers under
`container`. Analysis stages, coding passes, syntax records, packers, tables,
and native-reference infrastructure are private.

Buffered ATRAC3plus encoding (`encode_to_vec`) drives the streaming encoder. The
CLI likewise uses only the crate-root façades and owns WAV decoding, temporary
files, and atomic destination replacement.

## Dependency direction

The intended dependency flow is:

```text
tables / entropy primitives
          ↑
analysis → coding → typed frame syntax → bitstream packing
                 ↑
          encoder orchestration
                 ↓
             container I/O
```

The exact directory shape differs between the codecs because ATRAC3 has two
isolated core strategies (clean and DBA), while ATRAC3plus has a larger recovered
pipeline. The boundary rules are the same:

- coding does not import bitstream packing;
- bitstream packing does not import encoder orchestration;
- tables do not import higher layers;
- codec configuration is represented by validated profiles before it reaches a
  frame core;
- the ATRAC3plus production packer accepts owned typed syntax, never native
  object windows or offsets.

Integration tests in each codec crate scan these source boundaries so a reverse
dependency fails the normal workspace test command.

The selected architecture-focused clippy gate is:

```sh
cargo clippy --workspace --all-targets -- \
  -D clippy::large_enum_variant -D clippy::type_complexity
```

The recovered codec arithmetic intentionally retains a broader backlog of
style-level clippy findings; those are outside this structural refactor unless
they identify a layer or representation problem.

## Reference infrastructure

ATRAC3plus retains native function names when they help trace recovered
algorithms. Native offsets and object-window serializers live behind
`cfg(any(test, debug_assertions))` as a differential oracle. Release builds omit
that reference module and the offset-backed packer structures. The production
driver builds `FrameSyntax` directly from coding outputs and checks typed versus
reference packing only in debug/test builds.

## Behavioral invariants

Architecture changes are gated by compact end-to-end parity fixtures for every
supported profile. Those fixtures pin encoded bytes, RIFF headers, priming and
flush schedules, malformed-input behavior, sink failures, and ATRAC3's explicit
silence-frame fallback policy. See [encoder usage](encoder-usage.md) for the
public PCM/container contract and [parity baseline](parity-baseline.md) for the
acceptance criteria.
