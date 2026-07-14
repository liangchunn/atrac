# ATRAC crate architecture refactor plan

Scope: `crates/at3`, `crates/at3p`, and the parts of `crates/cli` that consume them  
Primary constraint: preserve encoded output and container behavior while changing internal architecture

## 1. Purpose

This plan turns the architecture review of `at3` and `at3p` into a sequence of small, verifiable changes. The goal is not a wholesale rewrite. The goal is to establish clear public APIs and directed internal dependencies while retaining the byte-exact and perceptual behavior already recovered from the native implementation.

The main outcomes are:

1. One validated configuration/profile model per codec.
2. A small, intentional public API for each crate.
3. One streaming implementation per codec, with buffered helpers built on top.
4. Explicit pipeline boundaries: analysis, coding, frame syntax, and bitstream packing.
5. Removal of fake native-memory windows from the `at3p` production path.
6. Separation of the clean and DBA implementations inside `at3`.
7. Native-layout and reverse-engineering compatibility code retained only as reference/test infrastructure.
8. One final, one-time external full-corpus encoding sweep against the supplied SHA-1 manifests.

## 2. Non-goals

- Do not change codec algorithms merely to make them look more idiomatic.
- Do not intentionally change encoded bytes, priming, flush scheduling, fallback policy, or RIFF headers.
- Do not merge ATRAC3 and ATRAC3plus signal-processing implementations.
- Do not create a broad shared crate until repeated, stable abstractions are demonstrated.
- Do not rename all native-derived functions in one mechanical pass.
- Do not delete native-offset adapters until the typed replacement has parity coverage.
- Do not mix architecture changes with unrelated optimizations or clippy cleanup.
- Do not place full-corpus sweep inputs, generated `.at3` files, logs, or SHA manifests in the repository.

## 3. Architectural rules

The refactor should follow these rules throughout:

- Invalid configuration should be unrepresentable after construction.
- The public API should expose codec concepts, not reverse-engineered memory layouts.
- Dependency flow should be one-way:

  ```text
  tables / entropy primitives
            ↑
  analysis → coding → frame syntax → bitstream
                   ↑
             encoder orchestration
                   ↓
               container I/O
  ```

- Buffered encoding must be a convenience wrapper over streaming encoding, not a second implementation.
- Native-memory offsets may exist in an oracle/reference adapter, but not as the production interface between coding and packing.
- Public errors should be stable and meaningful; internal stage errors may remain detailed and native-oriented.
- Every structural change must be protected by tests at the boundary being changed.

## 4. Target public APIs

The precise names can change during implementation, but both crates should converge on the same broad shape.

### `at3`

```rust
pub use config::{Atrac3Config, Atrac3Profile, ChannelMode};
pub use encoder::{Atrac3Encoder, EncodeError, EncodeProgress, EncodeSummary};
```

Expected primary lifecycle:

```rust
let profile = Atrac3Profile::new(bitrate_kbps, channels)?;
let mut encoder = Atrac3Encoder::new(writer, profile, input_sample_frames)?;
encoder.push_pcm(&channels)?;
let (writer, summary) = encoder.finish()?;
```

### `at3p`

```rust
pub use profile::{Atrac3plusProfile, Atrac3plusRate, ChannelMode};
pub use encoder::{Atrac3plusEncoder, EncodeError, EncodeProgress, EncodeSummary};
```

The normal API must not require callers to construct `CodingParams`, select a computed path, refer to `352`-named schedulers, or interact with native handle/object state.

### Shared API behavior

- Profiles have private fields and read-only accessors.
- Profile construction validates the complete tuple, not only bitrate.
- `new` returns `Result` when configuration or output sizing can fail.
- `push_pcm` accepts a channel slice and validates channel count and chunk length.
- `finish` consumes the encoder and returns the writer plus a summary.
- Progress callbacks are optional convenience methods around the same implementation.
- Buffered helpers, if retained, should be named plainly, such as `encode_to_vec`, and drive the streaming encoder internally.

## 5. Target internal structures

The end state should resemble:

```text
crates/at3/src/
  lib.rs
  config.rs
  encoder.rs
  error.rs
  schedule.rs
  container.rs
  core/
    mod.rs
    clean.rs
    dba.rs
    analysis/
    coding/
    syntax.rs
    bitstream/
  tables/

crates/at3p/src/
  lib.rs
  profile.rs
  encoder.rs
  error.rs
  schedule.rs
  container/
  pipeline/
    mod.rs
    analysis/
    coding/
    syntax.rs
  entropy/
  bitstream/
  reference/
    native_api.rs
    native_layout.rs
  tables/
```

This is a responsibility map, not a requirement to create every directory immediately. Files should move only when the new boundary has a typed interface and tests.

## 6. Execution phases

### Phase 0: Establish the parity baseline

Objective: make architectural movement safe before changing APIs or representations.

Tasks:

- Record the current workspace test command and expected result.
- Add integration-level fixtures for both codecs covering:
  - each supported channel mode;
  - each encoder strategy in `at3` (clean and DBA);
  - representative low, middle, and high rates in `at3p`;
  - exact-block and partial-final-block input lengths;
  - priming and flush boundaries;
  - empty/too-short and malformed input errors;
  - sink write failures at header and payload stages;
  - the existing ATRAC3 silence-frame fallback behavior.
- Store compact expected hashes or checked-in small fixtures rather than embedding large byte arrays in source.
- Add tests that compare streaming and buffered output where both APIs currently exist.
- Identify source comments that reference absent `docs/*`, `decompiled/*`, or `tests/*` material. Either add the referenced evidence, update the link, or label it as external/archive-only.
- Document whether byte equality or perceptual equality is the acceptance criterion for each supported rate.

Acceptance criteria:

- `cargo test --workspace` passes.
- A single command can run the parity suite.
- Every production profile has at least one end-to-end encode test.
- Refactor commits can distinguish an intentional fixture update from accidental codec drift.

Suggested commits:

1. `test: add codec architecture parity fixtures`
2. `docs: inventory native evidence and parity guarantees`

### Phase 1: Create one validated profile model per crate

Objective: remove duplicated and permissive configuration logic before moving pipeline code.

#### `at3` tasks

- Introduce `Atrac3Profile` as the sole owner of:
  - bitrate;
  - input/output channel count;
  - frame byte count;
  - joint-stereo mode;
  - clean versus DBA strategy;
  - priming behavior;
  - internal encoder bitrate used by mono modes.
- Move the configuration matrix currently split between `ResolvedConfig`, `EncAlgo::from_sony_params`, `sony_frame_bytes`, and stream-level validation into `Atrac3Profile::new`.
- Change the frame encoder constructor to accept `Atrac3Profile`, removing inference from a bitrate plus boolean.
- Replace invalid-configuration defaults with a typed `UnsupportedProfile` error.
- Keep compatibility constructors temporarily as deprecated wrappers if external callers need migration time.

#### `at3p` tasks

- Replace publicly constructible `EncodeProfile` with a validated `Atrac3plusProfile` whose fields are private.
- Make `(bitrate, channels, sample_rate)` or a typed rate/channel combination the construction key.
- Merge profile row facts and encode-setting row facts behind the validated type.
- Make `CodingParams::for_profile` infallible only for a validated profile; remove silent fallback values.
- Centralize support checks so `payload`, `stream`, configuration serialization, and CLI code do not maintain separate bitrate lists.
- Add accessors for frame size, codec info, sample rate, channel count, and coding parameters.

#### CLI tasks

- Resolve profiles using the library API.
- Remove duplicated supported-rate constants once error rendering can obtain supported values from the profile layer.

Acceptance criteria:

- There is one support matrix per codec.
- No public constructor can create a profile with inconsistent bitrate, channels, frame bytes, sample rate, or codec info.
- The CLI and all encoder entry points use validated profiles.
- Existing output fixtures remain unchanged.

Suggested commits:

1. `refactor(at3): centralize profile validation`
2. `refactor(at3p): make profiles validated and immutable`
3. `refactor(cli): consume codec profile APIs`

### Phase 2: Define and enforce the public façades

Objective: stop treating implementation modules as public API.

Tasks:

- Add root-level re-exports for the intended profile, encoder, progress, summary, and error types.
- Change implementation modules from `pub mod` to `mod` or `pub(crate) mod` incrementally.
- Keep only deliberately supported container inspection functions public.
- Add `#![warn(missing_docs)]` only after the public surface is small enough to maintain.
- Decide the disposition of `at3p::encoder::api::PublicEncoderHandle`:
  - preferred: move it to `reference::native_api` and make it private by default;
  - alternative: expose it under a `native-api` feature and clearly mark it as a compatibility adapter;
  - do not leave `encode` and `flush_encode` as prominent public methods that always return “not implemented.”
- Add compile-time public API tests or an API snapshot tool if crate consumers outside the workspace exist.

Acceptance criteria:

- The CLI imports normal encoding types from each crate root.
- Tables, coding passes, native state structures, and packer leaves are no longer public by default.
- Public documentation presents one obvious encoding path.
- API compatibility decisions are recorded in a short migration note.

Suggested commits:

1. `refactor(at3): introduce crate-root encoder facade`
2. `refactor(at3p): introduce crate-root encoder facade`
3. `refactor(at3p): quarantine native handle compatibility API`

### Phase 3: Consolidate `at3p` around one streaming lifecycle

Objective: remove duplicated buffered, writer, mono, stereo, and progress implementations.

Tasks:

- Rename the generic schedule from `ComputedSchedule352` to a rate-neutral name such as `EncodeSchedule` once tests prove it is profile-independent.
- Make `Atrac3plusStreamEncoder` the sole owner of:
  - schedule state;
  - PCM chunk validation;
  - frontend/coding driver state;
  - frame emission;
  - header and payload writes;
  - progress and summary accounting.
- Replace stereo-pair-specific internals with one channel-slice implementation.
- Retain stereo convenience methods only as thin adapters.
- Implement buffered helpers by constructing a `Vec<u8>` sink and driving the stream encoder.
- Remove rate-specific dispatch matches whose arms all call the same implementation.
- Collapse duplicated `ComputedPayloadError`, `ComputedFileError`, and `ComputedWriteError` layers into:
  - one public `EncodeError` with meaningful stages;
  - detailed internal error enums connected through `source()`.
- Keep validation precedence tests where matching native behavior is a requirement.

Acceptance criteria:

- Only one code path performs scheduling and output-frame emission.
- Mono and stereo share the same internal lifecycle.
- Buffered output is byte-identical to streaming output for every tested profile.
- Profile and shape validation occur once.
- Progress behavior remains unchanged.

Suggested commits:

1. `refactor(at3p): generalize encode schedule naming`
2. `refactor(at3p): unify mono and stereo stream lifecycle`
3. `refactor(at3p): build buffered helpers on streaming encoder`
4. `refactor(at3p): consolidate public encode errors`

### Phase 4: Introduce typed ATRAC3plus frame syntax

Objective: define the real boundary between coding decisions and bitstream emission.

Create a typed representation along these lines:

```rust
struct FrameSyntax {
    groups: Vec<BlockGroupSyntax>,
    frame_bytes: usize,
}

struct BlockGroupSyntax {
    header: BlockHeader,
    channels: Vec<ChannelSyntax>,
    stereo: Option<StereoSyntax>,
}

struct ChannelSyntax {
    idwl: IdwlSyntax,
    idsf: IdsfSyntax,
    idct: IdctSyntax,
    spectral: SpectralSyntax,
    gain: GainSyntax,
    gha: GhaSyntax,
}
```

The exact structures should encode invariants such as active-band counts, mode-specific payloads, and predictor relationships rather than expose generic vectors and mode integers wherever practical.

Tasks:

- Inventory every value read by `pack_frame_at5` from `ObjectWindow`, `cfg`, `gainb`, and GHA arenas.
- Group those values by emitted bitstream section.
- Define typed syntax structures without changing the existing packer initially.
- Add a conversion from current `FramePrepackerState` to `FrameSyntax`.
- Add a conversion from `FrameSyntax` back to the native-window representation for parity testing.
- Write structural validation on `FrameSyntax` before packing:
  - channel/group counts;
  - active band extents;
  - mode/payload agreement;
  - predictor targets;
  - frame-size budget.
- Add property-style or table-driven tests for each syntax section.

Acceptance criteria:

- All values consumed by the packer have a named typed home.
- Native offsets are absent from the `FrameSyntax` API.
- `native window -> syntax -> native window` preserves all packer-relevant values.
- Existing full-frame output remains byte-identical.

Suggested commits:

1. `refactor(at3p): define typed frame syntax model`
2. `test(at3p): verify syntax/native-layout round trips`
3. `refactor(at3p): validate frame syntax before packing`

### Phase 5: Make the ATRAC3plus packer consume typed syntax

Objective: remove native-memory emulation from production packing.

Tasks:

- Change each bitstream section packer to consume its typed syntax structure.
- Move symbol grouping and Huffman cost primitives used by both coding and packing into `entropy`.
- Remove the `coding -> bitstream::group` dependency; both layers should depend on `entropy` instead.
- Implement a typed `pack_frame` orchestrator preserving the current family-major emission order.
- During migration, run both packers in tests:
  - current native-window packer;
  - new typed-syntax packer;
  - assert identical frame bytes and final bit cursor.
- Switch `ComputedFrameDriver` to produce and pack `FrameSyntax` directly.
- Move `ObjectWindow`, `FramePrepackerState`, native offset constants, and serializers into `reference::native_layout`.
- Remove `packer_bridge` from the production dependency graph. It may remain temporarily as the reference adapter.

Acceptance criteria:

- The production encoder performs no serialization into fake native object memory.
- The production packer contains no reads by native byte offset.
- Typed and reference packers agree for every parity fixture.
- `coding`, `bitstream`, and `encoder` follow the intended dependency direction.
- `packer_bridge.rs` is deleted or exists only under the reference/test boundary.

Suggested commits:

1. `refactor(at3p): extract shared entropy primitives`
2. `refactor(at3p): pack idwl idsf and idct from typed syntax`
3. `refactor(at3p): pack spectral gain and gha from typed syntax`
4. `refactor(at3p): switch production frame packing to typed syntax`
5. `refactor(at3p): move native layout bridge to reference support`

### Phase 6: Reshape the ATRAC3 pipeline

Objective: separate DSP, coding, packing, and encoder strategy without altering algorithms.

Tasks:

- Introduce a private frame-core interface, for example:

  ```rust
  enum EncoderCore {
      Clean(CleanEncoder),
      Dba(DbaEncoder),
  }

  impl EncoderCore {
      fn encode_frame(
          &mut self,
          pcm: PcmFrame<'_>,
          output: &mut [u8],
      ) -> Result<EncodedFrame, FrameEncodeError>;
  }
  ```

- Move QMF, MDCT, gain, and transient processing into an `analysis` boundary.
- Move tone extraction, bit allocation, and quantization into `coding`.
- Move packing helpers out of the `dsp` namespace.
- Split `dba.rs` by responsibility only after its internal inputs/outputs are typed:
  - DBA analysis/state;
  - channel conversion;
  - gain/MDCT scheduling;
  - tone/allocation coding;
  - DBA syntax/packing.
- Split `encode.rs` into clean strategy orchestration plus reusable stages.
- Replace integer return codes from the core with a typed internal error.
- Make silence-frame substitution an explicit stream policy:
  - preserve current behavior by default;
  - retain the frame error as diagnostic data where useful;
  - test which failures are eligible for fallback.
- Remove stale documentation that describes an old return type or incomplete phase.

Acceptance criteria:

- `dsp` contains only signal-processing components.
- Bitstream packing is not under `dsp`.
- Clean and DBA state are isolated behind `EncoderCore`.
- Unsupported configuration cannot reach the core.
- Frame errors are typed before any fallback decision.
- All existing `at3` fixtures remain unchanged.

Suggested commits:

1. `refactor(at3): introduce typed frame core errors`
2. `refactor(at3): isolate clean and dba encoder strategies`
3. `refactor(at3): separate analysis coding and bitstream modules`
4. `refactor(at3): make fallback an explicit stream policy`

### Phase 7: Align the two crates where useful

Objective: offer a consistent developer experience without forcing codec internals together.

Tasks:

- Align naming and semantics for:
  - progress phase and counters;
  - stream summaries;
  - write-stage errors;
  - PCM chunk expectations;
  - `finish` ownership behavior.
- Consolidate duplicate CLI progress rendering into one generic renderer.
- Consider a small `atrac-common` crate only if stable duplication remains after both façades settle. Good candidates are simple progress/summary value types; DSP and codec configuration are not candidates.
- Ensure both crates document sample format, sample rate, channel layouts, priming, and output container behavior consistently.

Acceptance criteria:

- The CLI can drive either codec through parallel lifecycle code.
- User-facing progress behaves consistently.
- No shared abstraction depends on codec-specific native details.
- A shared crate is added only with at least two real consumers and no speculative API.

Suggested commits:

1. `refactor(cli): share codec progress rendering`
2. `docs: align at3 and at3p encoder usage`
3. Optional: `refactor: extract stable atrac common types`

### Phase 8: Cleanup and enforce the architecture

Objective: prevent the old structure from growing back.

Tasks:

- Remove deprecated constructors and compatibility entry points after their migration window.
- Remove obsolete `computed_*`, `target_352_*`, and numbered-phase names where their responsibilities are now generic.
- Keep native function names where they provide algorithmic traceability, but put address/evidence narratives in reference documentation rather than public API docs.
- Break remaining files above an agreed size threshold when they contain more than one responsibility. Do not split generated tables merely to satisfy the threshold.
- Add dependency checks or review rules:
  - coding must not depend on bitstream;
  - bitstream must not depend on encoder orchestration;
  - tables must not depend on higher layers;
  - reference/native-layout types must not appear in production encoder signatures.
- Run clippy and address architectural warnings such as excessive argument lists and type complexity after the boundaries are stable.
- Add crate-level architecture documentation and examples.

Acceptance criteria:

- Public rustdoc shows a compact, coherent API.
- No production path references `ObjectWindow` or native object offsets.
- No duplicated profile support matrices remain.
- No production method advertised as an encoder stub always returns “not implemented.”
- Workspace tests, parity fixtures, formatting, and selected clippy checks pass.

### Phase 9: Run the one-time external full-corpus SHA-1 sweep

Objective: as the final step, after every implementation, cleanup, test, and documentation phase is complete, encode the complete external mono/stereo song corpus with every supported bitrate/configuration and compare every generated `.at3` file against the supplied SHA-1 manifests.

This is an expensive terminal sanity check, not a development-loop test. It must be reserved until the architecture refactor is otherwise complete and the exact release binary intended for sign-off has been built. Do not run preliminary or partial versions of this corpus sweep during earlier phases.

External corpus root:

```text
/Users/liangchun/Desktop/sweep_all
```

Authoritative manifests:

```text
/Users/liangchun/Desktop/sweep_all/encoded_atrac3.sha1
/Users/liangchun/Desktop/sweep_all/encoded_atrac3plus.sha1
```

The manifests define this exact matrix, with 131 source songs per configuration:

| Codec | Input | Bitrates (kbps) | Expected files |
|---|---|---|---:|
| ATRAC3 | mono | 52, 66 | 262 |
| ATRAC3 | stereo | 66, 105, 132 | 393 |
| ATRAC3plus | mono | 32, 48, 64, 96, 128 | 655 |
| ATRAC3plus | stereo | 48, 64, 96, 128, 160, 192, 256, 320, 352 | 1,179 |
| **Total** | | 19 codec/input/rate configurations | **2,489** |

The expected per-codec manifest counts are therefore:

- `encoded_atrac3.sha1`: 655 files.
- `encoded_atrac3plus.sha1`: 1,834 files.

Preconditions:

- Phases 0 through 8 are complete.
- All workspace tests, parity fixtures, formatting checks, and agreed clippy checks pass.
- The working tree contains no unreviewed source changes.
- The final release executable has been built from the exact revision being signed off:

  ```bash
  cargo build --release --locked
  ```

- `/Users/liangchun/Desktop/sweep_all/mono` and `/Users/liangchun/Desktop/sweep_all/stereo` each contain the expected 131 source WAV files.
- Fresh external output directories are used. They must be outside the repository, for example:

  ```text
  /Users/liangchun/Desktop/sweep_all/encoded_atrac3_new
  /Users/liangchun/Desktop/sweep_all/encoded_atrac3plus_new
  ```

Execution:

1. Record the repository revision and release binary SHA-1 in an external log under `/Users/liangchun/Desktop/sweep_all`; do not add the log to the repository.
2. Ensure both `_new` output directories are absent or empty so stale files cannot produce false success.
3. Run the existing external recipes:

   ```text
   /Users/liangchun/Desktop/sweep_all/commands/atrac3.txt
   /Users/liangchun/Desktop/sweep_all/commands/atrac3plus.txt
   ```

   These recipes invoke `target/release/atrac encode` and write only beneath the two external `_new` output directories.
4. Confirm that every encode process succeeded and that output counts are exactly 655 ATRAC3 files and 1,834 ATRAC3plus files.
5. Validate all generated files from inside their respective output roots so the `./mono/<bitrate>/...` and `./stereo/<bitrate>/...` paths in the manifests resolve correctly:

   ```bash
   cd /Users/liangchun/Desktop/sweep_all/encoded_atrac3_new
   shasum -a 1 -c ../encoded_atrac3.sha1

   cd /Users/liangchun/Desktop/sweep_all/encoded_atrac3plus_new
   shasum -a 1 -c ../encoded_atrac3plus.sha1
   ```

6. Check for unexpected extra `.at3` files in either output tree in addition to confirming every manifest entry.
7. Record only the external run summary: revision, binary hash, start/end time, file counts, and zero/nonzero mismatch count. Do not copy encoded artifacts into the repository.
8. Confirm `git status --short` is unchanged from before the sweep.

Failure policy:

- Any encoder failure, missing file, extra file, filename mismatch, or SHA-1 mismatch blocks completion of the architecture refactor.
- Preserve the failing external artifacts and mismatch log for diagnosis; do not update either authoritative manifest to accept new output unless an encoded-output change was separately reviewed and explicitly approved.
- Because this sweep is intentionally reserved as a one-time final check, do not consume it as an exploratory test. If it fails, reopen the relevant implementation phase, fix and verify with focused tests, and schedule any necessary full rerun deliberately as a new final sign-off attempt.

Acceptance criteria:

- All 655 ATRAC3 manifest entries report `OK`.
- All 1,834 ATRAC3plus manifest entries report `OK`.
- No unexpected `.at3` files exist in the fresh output trees.
- All 131 mono and 131 stereo source songs were encoded at every bitrate applicable to their codec/channel configuration.
- No sweep artifact or log was created inside the repository.
- The repository working tree is unchanged by the sweep.
- The external summary identifies the exact signed-off revision and release binary.

## 7. Verification matrix

Every phase should run the relevant subset of this matrix.

| Area | Verification |
|---|---|
| Build | `cargo check --workspace --all-targets` |
| Unit/integration | `cargo test --workspace` |
| Formatting | `cargo fmt --all -- --check` |
| Lints | `cargo clippy --workspace --all-targets` with an agreed warning baseline |
| ATRAC3 profiles | All supported mono/stereo bitrate combinations |
| ATRAC3 strategies | Clean and DBA paths |
| ATRAC3plus profiles | All supported mono and stereo rows |
| Scheduling | Exact block, partial block, 1680/1681 flush boundary, minimum input |
| Streaming | Buffered/streamed output equality and bounded PCM storage |
| Containers | Header fields, data lengths, seek/read inspection, sink failures |
| Bitstream migration | Native-window packer versus typed-syntax packer byte equality |
| Error behavior | Unsupported profile, wrong channels, wrong chunk size, incomplete input |
| Final external sign-off | One-time 2,489-file SHA-1 sweep against `encoded_atrac3.sha1` and `encoded_atrac3plus.sha1` |

For a phase to merge, its relevant rows must pass without unexplained fixture changes.

## 8. Pull request and commit strategy

- Prefer one architectural boundary per pull request.
- Keep moves separate from behavioral edits where possible so diffs remain reviewable.
- Do not combine both crates in a single commit unless the change is deliberately shared, such as CLI progress rendering.
- Include in each pull request:
  - the boundary being introduced or removed;
  - the old and new dependency direction;
  - parity commands run;
  - any intentional public API changes;
  - evidence that encoded output is unchanged.
- Use temporary adapters to keep intermediate commits buildable; delete them in a later explicit commit.

## 9. Risks and mitigations

### Bit-exact output changes

Risk: typed representations accidentally normalize native quirks or change emission order.

Mitigation: dual-run old and new packers over the same fixtures until the production switch is complete.

### Configuration behavior changes

Risk: stricter constructors reject a configuration previously accepted accidentally.

Mitigation: inventory all current callers, distinguish supported profiles from malformed manual construction, and provide typed migration errors.

### Loss of reverse-engineering traceability

Risk: idiomatic names obscure correspondence with native functions and offsets.

Mitigation: retain native names and offset maps in `reference` documentation and adapters; separate traceability from production data flow.

### Oversized migration

Risk: moving files and changing representations simultaneously creates unreviewable diffs.

Mitigation: introduce typed interfaces in place, add adapters, switch consumers, then move files.

### Premature cross-crate abstraction

Risk: a shared crate couples two codecs with genuinely different schedules and algorithms.

Mitigation: align public semantics first; extract only small abstractions proven identical after both refactors.

## 10. Completion definition

The architecture refactor is complete when:

- Both crates expose a small root-level encoder API.
- All accepted profiles are represented by validated, immutable values.
- Streaming is the single encoding implementation in each crate.
- `at3p` production coding passes typed frame syntax directly to the bitstream packer.
- Native-memory windows and native handle emulation are isolated under reference/test support.
- `at3` clean and DBA strategies have separate state and pipeline orchestration.
- Internal dependencies follow the documented direction.
- All supported profiles pass the parity matrix.
- The CLI uses the public façades rather than internal modules.
- Architecture and usage documentation match the code that is shipped.
- The final one-time external corpus sweep passes all 2,489 SHA-1 checks without creating repository artifacts.
