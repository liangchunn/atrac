//! Config-window bridge (docs/11 Phase 1, bridge 1.7): build the shared block
//! config window `cfg[0, 0x400)` for the 352 kbps stereo path from 352 constants
//! + per-frame fields, without any captured config window.
//!
//! `cfg` is the shared block config `*(obj + 4)`: one heap block per block group,
//! identical `cfg_blob` sha1 for both objects of every call). It is the same
//! structure the calc composition calls "ctx" (`src/coding/calc_block.rs`, entry
//! `ctx_*` fields) and the coding bridge models as `config_*`
//! (`src/encoder/coding_bridge.rs`). `pack_frame_at5` reads it via `cfg_u32`.
//!
//! # Native sources of truth
//!
//! * **Constants (byte-invariant across all 77 output-bearing core calls 7..83).**
//!   The orchestrator diffed all 77 captured `cfg_blob`s byte-by-byte; the words
//!   baked below are byte-identical on every call, which is the native evidence
//!   for treating them as 352 constants. Cross-checked against the Slice D calc
//!   entry capture (`tests/coding_bridge_calc.rs` asserts `config_50=[0;16]`,
//!   `config_90=1`, `config_a8=2`, `config_ac=44100`, `config_b8=10`,
//!   `config_c4=32`, `config_b0`, `config_c0` byte-exact). Call-7 nonzero
//!   constants: `0x90=1, 0xa0=1, 0xa8=2, 0xac=44100, 0xb0=32, 0xb4=32, 0xb8=10,
//!   0xbc=16, 0xc0=16, 0xc4=32, 0xc8=16, 0xcc=1, 0xd0=1, 0x1e0=16379, 0x1e8=30,
//!   0x1ec=2193 (0x891), 0x1f0=16`. Everything else in `[0, 0x400)` is zero on
//!   call 7 except the per-frame / deferred regions below.
//! * **Per-frame `0xb0` (active band count) and `0xc0` (level groups).** Recomputed
//!   per frame by zeroth (`decompiled/libatrac.c` ~36730-36745: downward scan of
//!   the `+0x1b5f8` word-length rows, then `cfg+0xc0 = g_a_x_at5[active]+1`).
//!   Already computed live by `assemble_calc_frame_entry_at5`
//!   (`compute_zeroth_active_band_counts_at5`, `src/encoder/coding_bridge.rs`);
//!   ([`CfgPerFrame352::active_b0`] / [`CfgPerFrame352::level_groups_c0`]) so the
//!   builder is not silently a constant emitter.
//! * **`0x1e4` (calc epilogue word).** Sign-extended `s_12e`, computed live as
//!   `CalcFrameOutput.ctx_field_1e4` by `calc_channel_block_frame_at5`
//!   (`src/coding/calc_block.rs`, `ctx_field_1e4 = i32::from(s_12e)`). Call-7
//!   value: 16364. Taken as [`CfgPerFrame352::bits_1e4`].
//! * **Stereo config side data** (group 1 `0x48` head / `0x4c` inner /
//!   `0x50 + k*4`; group 2 `0x00` head / `0x04` inner / `0x08 + k*4`). Parameterized
//!   as per-frame input. The orchestrator's 77-call diff found these nonzero ONLY
//!   on calls 59/60/66 (group-1 words `0x48/0x4c/0x64/0x68/0x88`); group 2
//!   (`0x00..0x48`) is zero on ALL calls; both groups are ZERO at call 7. The
//!   native writer stage of these flags is not yet identified (recorded as a
//!   Phase-2 evidence question, not owned here).
//!
//! # Packer-read cfg surface (every `cfg_u32` read in `src/bitstream/frame.rs`)
//!
//! * block header: `0xa0` (2 bits), `0xc4`-1 (5 bits), `0x118` (1 bit)
//! * gates/counts: `0xb0` (quant-unit count), `0x90` (bandwidth mode),
//!   `0xc0` (IDSPCQU table index base + stereo flag run length), `0xc8`
//!   (gainB count), `0xc4` (config count)
//! * stereo config side data: group 1 `0x48`/`0x4c`/`0x50 + k*4` (k < 16);
//!   group 2 `0x00`/`0x04`/`0x08 + k*4` (k < 16)
//! * post-payload: `0x94` flag, `0x98`, `0x9c` (all zero on all calls)
//! * differential GHA IDSF mode-2 predictor map: `0x11c + (base+w)*4` — read ONLY
//!   when that GHA leaf dispatches; native output frame 0 (call 7) does NOT select
//!   it (differential leaves fire on calls 8/10/76).
//! * `nblk = *(cfg + 0xa8)` is modeled separately as `BlockGroup.nblk`.
//!
//! # DEFERRED regions (left zero; owner + evidence, in the style of
//! `src/reference/native_layout.rs`'s CUT list)
//!
//! These three regions are the ONLY bytes the captured call-7 window carries that
//! this builder does NOT emit. The composition harness (`tests/composed_frame.rs`)
//! empirically proves they are packer-UNREAD at call 7 (the packed frame is
//! byte-exact with them zeroed).
//!
//! * **Predictor map `0x11c..0x17c`** — the differential GHA IDSF mode-2 predictor
//!   map. Written natively during zeroth by `calc_nbits_for_gha_at5` (only call
//!   site `zeroth_bit_allocation_at5`, native `0x44c30`). This builder still emits
//!   ZEROS here (keeping the cfg exact-diff-set test intact); bridge **1.5** now
//!   COMPUTES the map (`calc_nbits_for_gha_at5` compact map) and layers it into the
//!   shared cfg at composition via `serialize_gha_cfg_map`. At call 7 it holds
//!   `[0,2,2,4,5,6,8,9,10,11,10,11]` then zeros, and is packer-UNREAD (call 7 does
//!   not dispatch the differential GHA leaf).
//! * **Float scratch `0x234..0x2f4` (48 f32)** — allocation-stage scratch.
//!   Packer-unread; native writer unidentified (referenced nowhere in src/ or
//!   docs/). *Owner*: unassigned (Phase-2 evidence question).
//! * **Float scratch `0x374..0x3f4` (32 f32)** — allocation-stage scratch.
//!   Packer-unread; native writer unidentified. *Owner*: unassigned (Phase-2
//!   evidence question).

#[cfg(any(test, debug_assertions))]
use crate::{bitstream::frame::ObjectWindow, coding::allocation::zeroth_band_shape_counts_at5};

/// Byte length of the config window this builder emits: `[0, 0x400)`.
pub const CFG_WINDOW_LEN: usize = 0x400;

// --- Stereo config side data offsets -------------------------------------

/// Group 2 head word (`0x00`).
pub const OFFSET_GROUP2_HEAD: usize = 0x00;
/// Group 2 inner word (`0x04`).
pub const OFFSET_GROUP2_INNER: usize = 0x04;
/// Group 2 per-k words base (`0x08 + k*4`, k < 16).
pub const OFFSET_GROUP2_KBASE: usize = 0x08;
/// Group 1 head word (`0x48`).
pub const OFFSET_GROUP1_HEAD: usize = 0x48;
/// Group 1 inner word (`0x4c`).
pub const OFFSET_GROUP1_INNER: usize = 0x4c;
/// Group 1 per-k words base (`0x50 + k*4`, k < 16).
pub const OFFSET_GROUP1_KBASE: usize = 0x50;

// --- Constant / per-frame scalar offsets ---------------------------------

/// Bandwidth mode (`0x90`, constant 1).
pub const OFFSET_90: usize = 0x90;
/// Post-payload flag (`0x94`, zero on all calls).
pub const OFFSET_94: usize = 0x94;
/// Post-payload word (`0x98`, zero on all calls).
pub const OFFSET_98: usize = 0x98;
/// Post-payload word (`0x9c`, zero on all calls).
pub const OFFSET_9C: usize = 0x9c;
/// Block-header 2-bit selector (`0xa0`). Native law (`init_channel_block_at5`,
/// decompile 34413): `*(cfg + 0xa0) = (uint)(param_5 != 1)` where param_5 is the
/// channel count — 1 at stereo, 0 at mono. Oracle: 1 on all 77 stereo 352
/// states; 0 on all 77 `frame_prepacker_128_mono` states (`field_0xa0_u32`).
pub const OFFSET_A0: usize = 0xa0;
/// `nblk` source (`0xa8`) — modeled separately as `BlockGroup.nblk` but kept
/// here for cfg-window completeness (the native packer's `iVar19` read,
/// decompile 46182, IS this word). Native law (`atx_init_encode`, decompile
/// 48766): `*(cfg + 0xa8) = param_2` (the channel count) — 2 at stereo, 1 at
/// mono (`frame_prepacker_128_mono` `nblk_u32 == 1` on all 77 states).
pub const OFFSET_A8: usize = 0xa8;
/// Sample rate (`0xac`, constant 44100).
pub const OFFSET_AC: usize = 0xac;
/// Active band / quant-unit count (`0xb0`, per-frame; 32 at call 7).
pub const OFFSET_B0: usize = 0xb0;
/// `band_index` (`0xb4`; per-rate — 32 full-band, 29 at 192, 28 at 160;
/// docs/13 §3). The scale-factor / quant-unit count the sigproc tail writes
/// (`cfg+0xb4 = p5`). Packer-UNREAD (byte-invariant regardless of value);
/// threaded for honesty.
pub const OFFSET_B4: usize = 0xb4;
/// Constant `0xb8` (10) — the header shape count (`g_a_sg_shape_index_at5
/// [band_index - 1] + 1`); 10 at band_index in {28..32} (incl. 192's 29), 9 at
/// 27 (a Phase-3.3 concern, not this slice). Kept constant on the landed rates.
pub const OFFSET_B8: usize = 0xb8;
/// `band_count` (`0xbc`; per-rate — 16 full-band, 13 at 192, 12 at 160;
/// docs/13 §3). `g_a_x_at5[band_index] + 1`, the QMF/gain-group count the
/// sigproc tail writes (`cfg+0xbc = g_a_x_at5[p5] + 1`). Packer-UNREAD;
/// threaded for honesty.
pub const OFFSET_BC: usize = 0xbc;
/// Level groups / IDSPCQU table index base + stereo run length (`0xc0`,
/// per-frame; 16 at call 7).
pub const OFFSET_C0: usize = 0xc0;
/// Final word-length/config count (`0xc4`): 32 for quant-unit counts 29..=31;
/// otherwise copied from `cfg+0xb4` (28 at 160, 32 at full band).
pub const OFFSET_C4: usize = 0xc4;
/// Final gain-group count (`0xc8`): 16 for quant-unit counts 29..=31;
/// otherwise copied from `cfg+0xbc` (12 at 160, 16 at full band).
pub const OFFSET_C8: usize = 0xc8;
/// mode_cc gate word `0xcc` (`ms_seed_0xcc` in the docs/13 §0.2 stereo
/// init-word oracle; docs/14 §0.2 mono oracle): 1-ch `selector >= 0xf`,
/// 2-ch `selector >= 0x13` — MEASURED 0 at stereo 48/64 and mono 48, 1 at
/// every other shipped rate.
pub const OFFSET_CC: usize = 0xcc;
/// GHA-enable gate word `0xd0` (same oracles): 1-ch `selector > 0xe`, 2-ch
/// `selector > 0x12` — numerically identical to `0xcc` within each channel
/// mode (see `CodingParams::{mode_cc, gha_enabled}`).
pub const OFFSET_D0: usize = 0xd0;
/// Block-header 1-bit flag (`0x118`, constant 0).
pub const OFFSET_118: usize = 0x118;
/// Frame bit budget (`0x1e0`; per-rate — 16379 at 352, 14907 at 320; the frame
/// `budget = frame_bytes*8 - 2*block_count - 3`). Threaded per-rate (docs/13
/// §1.1); the sole native writer is `zeroth_bit_allocation_at5` (native
/// `0x423c0`), value pinned by `zeroth_budget_by_rate.ndjson`.
pub const OFFSET_1E0: usize = 0x1e0;
/// Calc epilogue word (`0x1e4`, per-frame `ctx_field_1e4`; 16364 at call 7).
pub const OFFSET_1E4: usize = 0x1e4;
/// Block selector (`0x1e8`; per-rate — 30 at 352, 29 at 320). Threaded per-rate
/// (docs/13 §1.1); value pinned by `init_words_by_rate.ndjson` /
/// `zeroth_budget_by_rate.ndjson`.
pub const OFFSET_1E8: usize = 0x1e8;
/// Constant `0x1ec` (2193 / 0x891) — rate-INDEPENDENT: `zeroth_budget_by_rate`
/// `cfg_1ec_u32 == 2193` at ALL NINE rates. Its native writer/law is
/// unidentified (registered follow-up, docs/13 §1.1), but the VALUE is
/// oracle-observed constant, so this stays a constant emitter.
pub const OFFSET_1EC: usize = 0x1ec;
/// Constant `0x1f0` (16) — oracle-observed 16 at 352 AND 320 only
/// (`zeroth_budget_by_rate` `cfg_1f0_u32`). The word is RATE-DEPENDENT below 320
/// (3/4/7/9/10/11/14 for 48..256), packer-unread, writer/law unidentified
/// (registered in docs/13 Appendix B for the §2.2/§2.1 256 bring-up). Emitting
/// the constant 16 stays observation-honest for the 352/320 slice; do NOT invent
/// a law.
pub const OFFSET_1F0: usize = 0x1f0;

/// Per-frame inputs the 352 config window depends on. Everything else in the
/// window is a 352 constant or a DEFERRED (zeroed) region — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgPerFrame352 {
    /// Zeroth active band count (`cfg+0xb0`); 32 at call 7. From
    /// `assemble_calc_frame_entry_at5` / the calc-entry `band_count_b0` word.
    pub active_b0: u32,
    /// Zeroth group count (`cfg+0xc0`); 16 at call 7. From
    /// `assemble_calc_frame_entry_at5` / the calc-entry `level_groups_c0` word.
    pub level_groups_c0: u32,
    /// Stereo group 1: (`0x48` head, `0x4c` inner, `0x50 + k*4` for k<16).
    /// ZERO at call 7; nonzero only on calls 59/60/66. Native writer stage not
    /// yet identified.
    pub stereo_group1: (u32, u32, [u32; 16]),
    /// Stereo group 2: (`0x00` head, `0x04` inner, `0x08 + k*4` for k<16). ZERO
    /// on ALL captured calls.
    pub stereo_group2: (u32, u32, [u32; 16]),
    /// Calc epilogue word (`cfg+0x1e4`); `CalcFrameOutput.ctx_field_1e4`
    /// (sign-extended `s_12e`); 16364 at call 7.
    pub bits_1e4: i32,
}

#[cfg(any(test, debug_assertions))]
fn put_u32(window: &mut ObjectWindow, offset: usize, value: u32) {
    window.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Build the shared 352 config window `cfg[0, 0x400)` from the 352 constants and
/// the per-frame fields. The window is `mem_offset == 0`, `0x400` bytes; the
/// three DEFERRED regions (predictor map `0x11c..0x17c`, float scratch
/// `0x234..0x2f4` and `0x374..0x3f4`) are left zero — see the module docs for
/// their owners and evidence. Byte-exact vs the captured call-7 `cfg_blob`
/// everywhere outside those deferred regions (proven in `tests/composed_frame.rs`).
///
/// calls [`build_cfg_window`] with the 352 selector (30), budget (16379),
/// full-band extent (band_index 32 / band_count 16), and stereo channel
/// count (2).
#[cfg(any(test, debug_assertions))]
pub fn build_cfg_window_352(per_frame: &CfgPerFrame352) -> ObjectWindow {
    build_cfg_window(per_frame, 30, 16379, 0x20, 16, 2)
}

/// Build the shared config window `cfg[0, 0x400)` for a per-rate stereo path
/// (docs/13 §1.1), threading the block `selector` (`cfg+0x1e8`) and frame bit
/// `budget` (`cfg+0x1e0`) in place of the pinned 352 constants. Every other word
/// is a rate-independent constant / per-frame field / DEFERRED (zeroed) region
/// (see the module docs and the `0x1ec`/`0x1f0` offset docs). At (30, 16379,
/// 32, 16) this is byte-identical to the shipped 352 builder.
///
/// `band_index` (`cfg+0xb4`) and `band_count` (`cfg+0xbc`) are the per-frame
/// effective band extent (docs/13 §3): 32/16 full-band, 29/13 at 192, and
/// 28/12 at 160. The native zeroth epilogue uses unsigned
/// `(band_index - 29) < 3`: 29..=31 round up to header counts 32/16, while all
/// other valid counts copy `cfg+0xb4`/`cfg+0xbc` into `cfg+0xc4`/`cfg+0xc8`
/// (`zeroth_bit_allocation_at5`, decompile 36640-36660; native
/// `0x43134..0x43140`, `0x4386b..0x43883`). The shared law is
/// [`zeroth_band_shape_counts_at5`].
/// Shared-cfg `+0xb8` WLC subgroup shape count law (decompile 43502-43527,
/// `at5enc_sigproc` tail, Ghidra `0xcd03f` = native `0xbd03f` =
/// `g_a_sg_shape_index_at5- 1`): `g_a_sg_shape_index_at5[band_index - 1] + 1`
/// evaluated on the PER-FRAME effective (post `+0x1dc & 0x7c` override)
/// band_index, else 0 when band_index < 1. Values: band_index 27 → sg[26] = 8
/// → **9** (128 kbps, runtime-pinned by `band_extent_by_rate.ndjson`
/// `budget_store.cfg_b8_u32`); band_index 28/29/32 → sg[27]/sg[28]/sg[31] = 9
/// → **10** (160/192/256/320/352 and both 192 override states — why the prior
/// hardcode 10 held). Mirrors [`sigproc_band_limit_writeback_at5`]'s
/// `header_shape_count`.
pub fn cfg_shape_count_b8(band_index: u32) -> u32 {
    if band_index == 0 {
        0
    } else {
        u32::from(crate::tables::at5::sg_shape_index_at5()[band_index as usize - 1]) + 1
    }
}

/// `channel_count` (docs/14 §1.1 slice 3) threads the two channel-mode words:
/// `cfg+0xa0 = (channel_count != 1) as u32` (native writer
/// `init_channel_block_at5`, decompile 34413: `*(cfg + 0xa0) = (uint)(param_5 !=
/// 1)`) and `cfg+0xa8 = channel_count` (native writer `atx_init_encode`,
/// decompile 48766: `*(cfg + 0xa8) = param_2`). Measured: 1/2 on all 77 stereo
/// 352 states, 0/1 on all 77 `frame_prepacker_128_mono` states. At
/// `channel_count == 2` the emitted words are the former hardcoded 1/2, so every
/// stereo path is byte-identical.
#[cfg(any(test, debug_assertions))]
pub fn build_cfg_window(
    per_frame: &CfgPerFrame352,
    selector: u32,
    budget: i32,
    band_index: u32,
    band_count: u32,
    channel_count: u32,
) -> ObjectWindow {
    let mut window = ObjectWindow::new(0, vec![0u8; CFG_WINDOW_LEN]);

    // Rate-independent constants (byte-invariant across all 77 output-bearing
    // 352 calls; identical at 320 per init_words/zeroth_budget oracles).
    put_u32(&mut window, OFFSET_90, 1);
    // Channel-mode words: block-header selector `(channel_count != 1)` (decompile
    // 34413) and the `nblk` word `channel_count` (decompile 48766) — 1/2 at
    // stereo, 0/1 at mono (`frame_prepacker_128_mono` field_0xa0/nblk oracle).
    put_u32(&mut window, OFFSET_A0, u32::from(channel_count != 1));
    put_u32(&mut window, OFFSET_A8, channel_count);
    put_u32(&mut window, OFFSET_AC, 44100);
    // Per-FRAME effective band extent (packer-unread; docs/13 §3.1 slice 3): the
    // live driver threads this call's post-`+0x1dc & 0x7c`-override band_index /
    // band_count (29/13 or 32/16 per frame at 192; static 28/12 at 160 and
    // 32/16 at 256/320/352).
    put_u32(&mut window, OFFSET_B4, band_index);
    // 0xb8 shape count is the per-FRAME `g_a_sg_shape_index_at5[band_index-1]+1`
    // law (see `cfg_shape_count_b8`): 9 at band_index 27 (128), 10 at bl 28/29/32
    // (160/192/256/320/352 and both 192 override states — table indices 27/28/31
    // are all 0x09), equalling the sigproc writeback header_shape_count.
    put_u32(&mut window, OFFSET_B8, cfg_shape_count_b8(band_index));
    put_u32(&mut window, OFFSET_BC, band_count);
    // Header word-length/group counts: 29..=31 round up to 32/16; all other
    // valid extents copy the effective cfg+0xb4/cfg+0xbc pair (see fn doc).
    let band_shape = zeroth_band_shape_counts_at5(
        band_index as usize,
        band_index as usize,
        band_count as usize,
    );
    put_u32(&mut window, OFFSET_C4, band_shape.word_length_count as u32);
    put_u32(&mut window, OFFSET_C8, band_shape.group_count as u32);
    // mode_cc (`0xcc`) / GHA-enable (`0xd0`) gate words — packer-unread config,
    // MEASURED at BOTH channel modes: the docs/13 §0.2 stereo init-word oracle
    // (`init_words_by_rate.ndjson`: 0/0 at stereo 48/64 [sel 13/15], 1/1 at
    // 96-352 [sel >= 0x13]) and the docs/14 §0.2 mono oracle + the captured
    // `frame_prepacker_48_mono` call-7 window (0/0 at mono 48 [sel 13], 1/1 at
    // mono 64/96/128 [sel >= 0xf]). The two words carry numerically identical
    // laws within each channel mode (`sel >= 0xf` ≡ `sel > 0xe`, `sel >= 0x13`
    // ≡ `sel > 0x12`), mirroring `CodingParams::{mode_cc, gha_enabled}`.
    let low_selector_gates_open = if channel_count == 1 {
        selector >= 0xf
    } else {
        selector >= 0x13
    };
    put_u32(&mut window, OFFSET_CC, u32::from(low_selector_gates_open));
    put_u32(&mut window, OFFSET_D0, u32::from(low_selector_gates_open));
    // Per-rate: budget (`0x1e0`) and selector (`0x1e8`).
    put_u32(&mut window, OFFSET_1E0, budget as u32);
    put_u32(&mut window, OFFSET_1E8, selector);
    // Rate-independent 2193 (all nine rates) / oracle-observed 16 (352 + 320).
    put_u32(&mut window, OFFSET_1EC, 2193);
    put_u32(&mut window, OFFSET_1F0, 16);

    // Explicit zeros the packer reads (documented invariant-zero on all calls).
    put_u32(&mut window, OFFSET_94, 0);
    put_u32(&mut window, OFFSET_98, 0);
    put_u32(&mut window, OFFSET_9C, 0);
    put_u32(&mut window, OFFSET_118, 0);

    // Per-frame zeroth active counts.
    put_u32(&mut window, OFFSET_B0, per_frame.active_b0);
    put_u32(&mut window, OFFSET_C0, per_frame.level_groups_c0);

    // Per-frame calc epilogue word.
    put_u32(&mut window, OFFSET_1E4, per_frame.bits_1e4 as u32);

    // Stereo config side data (per-frame; all-zero at call 7).
    let (g1_head, g1_inner, g1_k) = &per_frame.stereo_group1;
    put_u32(&mut window, OFFSET_GROUP1_HEAD, *g1_head);
    put_u32(&mut window, OFFSET_GROUP1_INNER, *g1_inner);
    for (k, value) in g1_k.iter().enumerate() {
        put_u32(&mut window, OFFSET_GROUP1_KBASE + k * 4, *value);
    }
    let (g2_head, g2_inner, g2_k) = &per_frame.stereo_group2;
    put_u32(&mut window, OFFSET_GROUP2_HEAD, *g2_head);
    put_u32(&mut window, OFFSET_GROUP2_INNER, *g2_inner);
    for (k, value) in g2_k.iter().enumerate() {
        put_u32(&mut window, OFFSET_GROUP2_KBASE + k * 4, *value);
    }

    // DEFERRED regions left zero: predictor map 0x11c..0x17c (owner 1.5/1.1),
    // float scratch 0x234..0x2f4 and 0x374..0x3f4 (packer-unread, writer
    // unidentified). See module docs.

    window
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call7_per_frame() -> CfgPerFrame352 {
        CfgPerFrame352 {
            active_b0: 32,
            level_groups_c0: 16,
            stereo_group1: (0, 0, [0; 16]),
            stereo_group2: (0, 0, [0; 16]),
            bits_1e4: 16364,
        }
    }

    fn read_u32(window: &ObjectWindow, offset: usize) -> u32 {
        u32::from_le_bytes(window.bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn window_shape_is_0x400_at_offset_0() {
        let window = build_cfg_window_352(&call7_per_frame());
        assert_eq!(window.mem_offset, 0);
        assert_eq!(window.bytes.len(), CFG_WINDOW_LEN);
    }

    #[test]
    fn bakes_352_constants() {
        let window = build_cfg_window_352(&call7_per_frame());
        for (offset, value) in [
            (OFFSET_90, 1u32),
            (OFFSET_A0, 1),
            (OFFSET_A8, 2),
            (OFFSET_AC, 44100),
            (OFFSET_B4, 32),
            (OFFSET_B8, 10),
            (OFFSET_BC, 16),
            (OFFSET_C4, 32),
            (OFFSET_C8, 16),
            (OFFSET_CC, 1),
            (OFFSET_D0, 1),
            (OFFSET_1E0, 16379),
            (OFFSET_1E8, 30),
            (OFFSET_1EC, 2193),
            (OFFSET_1F0, 16),
        ] {
            assert_eq!(read_u32(&window, offset), value, "cfg[{offset:#x}]");
        }
    }

    #[test]
    fn threads_per_frame_fields() {
        let per_frame = CfgPerFrame352 {
            active_b0: 31,
            level_groups_c0: 15,
            stereo_group1: (7, 8, {
                let mut k = [0u32; 16];
                k[5] = 55;
                k
            }),
            stereo_group2: (0, 0, [0; 16]),
            bits_1e4: -3,
        };
        let window = build_cfg_window_352(&per_frame);
        assert_eq!(read_u32(&window, OFFSET_B0), 31);
        assert_eq!(read_u32(&window, OFFSET_C0), 15);
        assert_eq!(read_u32(&window, OFFSET_1E4), (-3i32) as u32);
        assert_eq!(read_u32(&window, OFFSET_GROUP1_HEAD), 7);
        assert_eq!(read_u32(&window, OFFSET_GROUP1_INNER), 8);
        assert_eq!(read_u32(&window, OFFSET_GROUP1_KBASE + 5 * 4), 55);
    }

    #[test]
    fn deferred_regions_left_zero() {
        let window = build_cfg_window_352(&call7_per_frame());
        for offset in (0x11c..0x17c).step_by(4) {
            assert_eq!(read_u32(&window, offset), 0, "predictor map {offset:#x}");
        }
        for offset in (0x234..0x2f4).step_by(4) {
            assert_eq!(read_u32(&window, offset), 0, "float scratch {offset:#x}");
        }
        for offset in (0x374..0x3f4).step_by(4) {
            assert_eq!(read_u32(&window, offset), 0, "float scratch {offset:#x}");
        }
    }
}
