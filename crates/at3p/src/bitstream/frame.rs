//! Whole-frame ATRAC3plus packer assembler (`pack_frame_at5`).
//!
//! This composes the trace-verified packer leaves (`pack_idwl.rs`,
//! `pack_idsf.rs`, `pack_idct.rs`, `pack_spectral.rs`, gain/GHA, and the frame
//! tail) in the exact native emission order of `atx_encode_core`
//! (`decompiled/libatrac.c` at native `0x559f0`, packing region
//! `0x55f25..0x5789a`). It performs no new packing arithmetic; every family is
//! dispatched through the existing native-offset dispatch tables and leaf
//! functions.
//!
//! # Pinned native frame layout (evidence)
//!
//! The packing gate is `*(atx_state + 0x18) == 0`. For the target 352 kbps
//! stereo profile the block-group count `*(atx_state + 0xc)` is `1` and that one
//! block group holds `nblk = *(block_cfg + 0xa8) = 2` blocks (the two audio
//! channels). Emission is therefore **family-major across the two blocks**, not
//! channel-major. Confirmed by live bit cursors on native output frame 0 (core
//! call 7): `idwl[11,69] idsf[112,240] idct[355,450] spectral=529
//! gain_side=15021 gha=15736 payload[16033,16306] post=16366 tail=16367`.
//!
//! Per frame:
//!  * Prologue: 1 reserved bit, value 0 (native advances the cursor over a
//!    zeroed buffer). cursor 0 -> 1.
//!  * For each block group (native `iStack_229cc` loop, once for 352 stereo):
//!    - Channel/block header:
//!      `*(cfg+0xa0)` (2 bits), `*(cfg+0xc4)-1` (5 bits), `*(cfg+0x118)` (1 bit).
//!    - IDWL section, per block `i in 0..nblk`:
//!      `*(obj+0x1c70c)` (2 bits) then dispatch
//!      `pf_pack_idwl_table[(*(obj+0x1c70c)&3) + (*(obj)&1)*4]`. Indices 3 and 7
//!      (ch0 vs ch1 mode-word 3) alias to the same native leaf
//!      `pack_idwl_3_at5`; channel behavior is keyed inside the leaf.
//!    - IDSF section (gated `*(cfg+0xb0) > 0`), per block:
//!      `*(obj+0x1c73c)` (2 bits) then dispatch
//!      `pf_pack_idsf_table[(*(obj+0x1c73c)&3) + (*(obj)&1)*4]`.
//!    - IDCT section (gated `*(cfg+0xb0) > 0`): `*(cfg+0x90)` (1 bit) then per
//!      block `*(obj+0x1074)` (1 bit), `*(obj+0x1078)` (2 bits), then dispatch
//!      `pf_pack_idct_table[(*(obj+0x1078)&3) + (*(obj)&1)*4]`.
//!    - Spectral payload + IDSPCQU tail (`0x56418`), the stereo config flags,
//!      the section-6 "gainB" flags (native 46786, `*(obj+8)+0x980`), the gain
//!      NGC/IDLEV/IDLOC side data (46866), the GHA header (47055), then per block
//!      the GHA side data (idloc/nwavs/freq/idsf/idam) + per-wave payload loop
//!      (`0x57346`), and the post-payload gate (`0x5750d`).
//!  * Frame tail: 2-bit marker `3`, byte align, `0x01` stuffing to `frame_bytes`
//!    via `writer::write_frame_tail`.
//!
//! The full frame is composed and is byte-exact against native output frame 0
//! (core call 7) for all 2048 bytes, hitting every captured native family cursor
//! (`idwl idsf idct spectral gain_side gha_chan gha_nwavs payload post_payload
//! tail_start`). The spectral descriptor slot is derived from the object state
//! (`*(obj+0x1074)`, selector `*(obj+0x1b578+qu*4)`, word length
//! `*(obj+0x1b5f8+qu*4)`); the GHA record wave arrays are read from the captured
//! `p1 = *(obj+0x14)` arena.
//!
//! The inter-channel *differential* GHA leaves are composed at frame level: GHA
//! NWAVS mode 2 (native `0x18dd0`), FREQ mode 1 (native `0x191b0`), and IDSF
//! mode 2 (native `0x1a840`) predict against the previous object's record arena
//! (`*(obj+0x28)`, resolved here via `previous_object`). NWAVS mode 2 deltas the
//! current active record's wave count against the previous object's record count.
//! FREQ mode 1 deltas the current wave's frequency against the previous object's
//! record wave at the same index (falling back to its last wave, else raw). IDSF
//! mode 2's per-wave predictor map lives in the shared block config at
//! `*(obj+4)+0x11c` (i32 entries): the map base advances by each ACTIVE record's
//! current wave count — inactive records neither consume entries nor advance the
//! base — each current wave `w` in an active record reads `map[base + w]`, and a
//! `-1` (`0xffffffff`) entry is the no-predictor sentinel. Sampled output frames
//! that select these leaves (core calls 8, 10, and 76) reproduce native output
//! byte-for-byte alongside 7/74/75.
//!
//! Coverage caveat: the IDAM family (gated `arena_root[1] == 0`) is never selected
//! guard in case scope widens.

use super::huffman::HuffmanEmitError;
use super::pack_gain::{
    Idlev3Fields, Idloc3Fields, IdlocRow, Ngc3Fields, PackGainError, pack_gain_idlev_0_at5,
    pack_gain_idlev_1_at5, pack_gain_idlev_2_at5, pack_gain_idlev_3_at5, pack_gain_idlev_4_at5,
    pack_gain_idlev_5_at5, pack_gain_idlev_6_at5, pack_gain_idloc_0_at5, pack_gain_idloc_1_at5,
    pack_gain_idloc_2_at5, pack_gain_idloc_3_at5, pack_gain_idloc_4_at5, pack_gain_idloc_5_at5,
    pack_gain_idloc_6_at5, pack_gain_ngc_0_at5, pack_gain_ngc_1_at5, pack_gain_ngc_2_at5,
    pack_gain_ngc_3_at5, pack_gain_ngc_4_at5, pack_gain_ngc_5_at5,
};
use super::pack_gha::{
    GhFreqRow, GhIdlocRow, GhIdsfRow, GhNwavsRow, PackGhaError, pack_gh_freq_0_at5,
    pack_gh_freq_1_at5, pack_gh_idloc_0_at5, pack_gh_idloc_1_at5, pack_gh_idsf_0_at5,
    pack_gh_idsf_1_at5, pack_gh_idsf_2_at5, pack_gh_idsf_3_at5, pack_gh_nwavs_0_at5,
    pack_gh_nwavs_1_at5, pack_gh_nwavs_2_at5, pack_gh_nwavs_3_at5,
};
use super::pack_idct::{
    Idct0Count, Idct0Row, PackIdctError, pack_idct_0_at5, pack_idct_1_at5, pack_idct_2_at5,
    pack_idct_4_at5,
};
use super::pack_idsf::{
    Idsf1Fields, Idsf2Fields, Idsf3Fields, Idsf4Fields, PackIdsfError, pack_idsf_0_at5,
    pack_idsf_1_at5, pack_idsf_2_at5, pack_idsf_3_at5, pack_idsf_4_at5, pack_idsf_5_at5,
    pack_idsf_6_at5,
};
use super::pack_idwl::{
    Idwl1Fields, Idwl2Fields, Idwl3Fields, Idwl4Fields, PackIdwlError, pack_idwl_0_at5,
    pack_idwl_1_at5, pack_idwl_2_at5, pack_idwl_3_at5, pack_idwl_4_at5, pack_idwl_5_at5,
};
use super::pack_spectral::{
    PackSpectralError, pack_spectral_descriptor_unit, pack_spectral_idspcqu_tail,
};
use super::writer::{BitWriter, BitWriterError};
use crate::pipeline::syntax::{FrameSyntax, IdctCountSyntax, IdctEncodingSyntax, IdctSyntax};
use crate::tables::at5::{isps_at5, nsps_at5};
use crate::tables::generated::{G_A_IDSPCBANDS_AT5, G_A_IDSPCQUS_AT5};
use crate::tables::spectral::SPECTRAL_DESCRIPTOR_SLOTS;

/// A captured memory window of a native object, indexable by native byte
/// offset. `mem_offset` is the object-relative offset the window begins at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectWindow {
    pub mem_offset: usize,
    pub bytes: Vec<u8>,
}

impl ObjectWindow {
    pub fn new(mem_offset: usize, bytes: Vec<u8>) -> Self {
        Self { mem_offset, bytes }
    }

    fn get_u32(&self, offset: usize) -> Option<u32> {
        let rel = offset.checked_sub(self.mem_offset)?;
        let slice = self.bytes.get(rel..rel + 4)?;
        Some(u32::from_le_bytes(slice.try_into().unwrap()))
    }

    fn get_u16(&self, offset: usize) -> Option<u16> {
        let rel = offset.checked_sub(self.mem_offset)?;
        let slice = self.bytes.get(rel..rel + 2)?;
        Some(u16::from_le_bytes(slice.try_into().unwrap()))
    }
}

/// One GHA record's wave array. Each wave entry is a native 0x10-byte struct;
/// the packer reads its idsf source (`+0x0`), idam source (`+0x4`), 5-bit phase
/// payload (`+0x8`), and frequency source (`+0xc`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhaWave {
    pub idsf: u32,
    pub idam: u32,
    pub phase: u32,
    pub freq: u32,
}

/// One native block object (channel) with the memory windows the packer reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectState {
    /// `*(obj)` native channel index; low bit is the dispatch channel parity.
    pub channel_index: u32,
    /// Object window `[0, 0x1110)`.
    pub range_a: ObjectWindow,
    /// Object window `[0x1b480, 0x1cc00)`.
    pub range_b: ObjectWindow,
    /// Shared block config `*(obj + 4)` window `[0, 0x400)`.
    pub cfg: ObjectWindow,
    /// Index within the group of the object `*(obj + 0x28)` points at, for the
    /// previous-state delta leaves (IDWL/IDSF/IDCT modes 4/5/7). `None` when the
    /// native previous pointer is not one of this group's objects.
    pub previous_index: Option<usize>,
    /// Secondary-gain "gainB" buffer `*(obj + 8)` window `[0, 0xb00)`: gain rows
    /// (stride `0x98`) and the section-6 flags at `+0x980/+0x984/+0x988+k*4`.
    pub gainb: ObjectWindow,
    /// GHA header arena_root `*(*(obj + 0x14))` window (nbands, extension flags).
    pub gha_arena: ObjectWindow,
    /// GHA record arena `p1 = *(obj + 0x14)` window (record int fields).
    pub gha_p1: ObjectWindow,
    /// `nrec = arena_root[2]` GHA records, each carrying its wave array.
    pub gha_records: Vec<Vec<GhaWave>>,
}

impl ObjectState {
    pub(crate) fn u32(&self, offset: usize) -> Result<u32, FrameAssemblyError> {
        self.range_a
            .get_u32(offset)
            .or_else(|| self.range_b.get_u32(offset))
            .ok_or(FrameAssemblyError::MissingObjectWord { offset })
    }

    pub(crate) fn cfg_u32(&self, offset: usize) -> Result<u32, FrameAssemblyError> {
        self.cfg
            .get_u32(offset)
            .ok_or(FrameAssemblyError::MissingConfigWord { offset })
    }

    fn u32_array(&self, offset: usize, count: usize) -> Result<Vec<u32>, FrameAssemblyError> {
        (0..count).map(|i| self.u32(offset + i * 4)).collect()
    }

    fn i32_array(&self, offset: usize, count: usize) -> Result<Vec<i32>, FrameAssemblyError> {
        Ok(self
            .u32_array(offset, count)?
            .into_iter()
            .map(|v| v as i32)
            .collect())
    }

    fn u16(&self, offset: usize) -> Result<u16, FrameAssemblyError> {
        self.range_a
            .get_u16(offset)
            .or_else(|| self.range_b.get_u16(offset))
            .ok_or(FrameAssemblyError::MissingObjectWord { offset })
    }

    fn u16_array(&self, offset: usize, count: usize) -> Result<Vec<u16>, FrameAssemblyError> {
        (0..count).map(|i| self.u16(offset + i * 2)).collect()
    }

    /// Word from the `*(obj + 8)` gainB window at object-relative byte `offset`.
    fn gainb_u32(&self, offset: usize) -> Result<u32, FrameAssemblyError> {
        self.gainb
            .get_u32(offset)
            .ok_or(FrameAssemblyError::MissingGainWord { offset })
    }

    /// Word from the GHA record arena `p1` at word index `idx`.
    fn p1_u32(&self, idx: usize) -> Result<u32, FrameAssemblyError> {
        self.gha_p1
            .get_u32(idx * 4)
            .ok_or(FrameAssemblyError::MissingGhaWord { index: idx })
    }

    /// Word from the GHA header arena_root at word index `idx`.
    pub(crate) fn arena_u32(&self, idx: usize) -> Result<u32, FrameAssemblyError> {
        self.gha_arena
            .get_u32(idx * 4)
            .ok_or(FrameAssemblyError::MissingGhaWord { index: idx })
    }
}

/// One block group `*(atx_state + 0x28 + group*0x44)` with its blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockGroup {
    /// `*(cfg + 0xa8)` block/channel count in this group.
    pub nblk: usize,
    pub objects: Vec<ObjectState>,
}

/// tests parse `frame0_prepacker_state.ndjson` into this shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramePrepackerState {
    pub frame_bytes: usize,
    /// `*(atx_state + 0xc)` block-group count.
    pub block_count: usize,
    pub groups: Vec<BlockGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameAssemblyError {
    MissingObjectWord {
        offset: usize,
    },
    MissingConfigWord {
        offset: usize,
    },
    MissingPreviousObject {
        block_index: usize,
    },
    UnsupportedDispatchIndex {
        family: &'static str,
        index: usize,
    },
    /// A family's in-frame ordering is not yet evidenced. Should never occur for
    /// the 352 stereo path (fully pinned); kept per the plan.
    UnpinnedOrdering {
        section: &'static str,
    },
    /// A spectral descriptor slot index derived from the object state falls
    /// outside the native 112-slot `g_aaa_hcspec` matrix.
    MissingSpectralSlot {
        slot_index: usize,
    },
    MissingGainWord {
        offset: usize,
    },
    MissingGhaWord {
        index: usize,
    },
    /// The GHA `nbands` symbol index has no native `g_a_gh_nbands_pack` entry.
    MissingNbandsSymbol {
        nbands: usize,
    },
    Idwl(PackIdwlError),
    Idsf(PackIdsfError),
    Idct(PackIdctError),
    Spectral(PackSpectralError),
    Gain(PackGainError),
    Gha(PackGhaError),
    Huffman(HuffmanEmitError),
    BitWriter(BitWriterError),
}

impl From<BitWriterError> for FrameAssemblyError {
    fn from(error: BitWriterError) -> Self {
        Self::BitWriter(error)
    }
}
impl From<PackIdwlError> for FrameAssemblyError {
    fn from(error: PackIdwlError) -> Self {
        Self::Idwl(error)
    }
}
impl From<PackIdsfError> for FrameAssemblyError {
    fn from(error: PackIdsfError) -> Self {
        Self::Idsf(error)
    }
}
impl From<PackIdctError> for FrameAssemblyError {
    fn from(error: PackIdctError) -> Self {
        Self::Idct(error)
    }
}
impl From<PackSpectralError> for FrameAssemblyError {
    fn from(error: PackSpectralError) -> Self {
        Self::Spectral(error)
    }
}
impl From<PackGainError> for FrameAssemblyError {
    fn from(error: PackGainError) -> Self {
        Self::Gain(error)
    }
}
impl From<PackGhaError> for FrameAssemblyError {
    fn from(error: PackGhaError) -> Self {
        Self::Gha(error)
    }
}
impl From<HuffmanEmitError> for FrameAssemblyError {
    fn from(error: HuffmanEmitError) -> Self {
        Self::Huffman(error)
    }
}

fn dispatch_index(mode_low_bits: u32, channel_parity: u32) -> usize {
    ((mode_low_bits & 3) + ((channel_parity & 1) << 2)) as usize
}

/// Assemble one whole ATRAC3plus frame from captured pre-packer state, walking
/// the pinned native emission order and composing the existing packer leaves.
///
pub fn pack_frame_at5(
    state: &FramePrepackerState,
    syntax: &FrameSyntax,
    writer: &mut BitWriter<'_>,
) -> Result<(), FrameAssemblyError> {
    pack_frame_at5_impl(state, Some(syntax), writer)
}

/// Temporary offset-driven parity oracle retained while payload families move
/// to owned frame syntax. Production packing must use [`pack_frame_at5`].
pub(crate) fn pack_frame_reference_at5(
    state: &FramePrepackerState,
    writer: &mut BitWriter<'_>,
) -> Result<(), FrameAssemblyError> {
    pack_frame_at5_impl(state, None, writer)
}

fn pack_frame_at5_impl(
    state: &FramePrepackerState,
    syntax: Option<&FrameSyntax>,
    writer: &mut BitWriter<'_>,
) -> Result<(), FrameAssemblyError> {
    // Frame prologue: one reserved bit, value 0.
    {
        writer.write_bits(0, 1)?;
        Ok::<(), FrameAssemblyError>(())
    }?;

    for (group_index, group) in state.groups.iter().enumerate() {
        let syntax_group = syntax.and_then(|syntax| syntax.groups().get(group_index));
        if syntax.is_some() && syntax_group.is_none() {
            return Err(FrameAssemblyError::UnpinnedOrdering {
                section: "typed_group_count",
            });
        }
        let cfg_source = &group
            .objects
            .first()
            .ok_or(FrameAssemblyError::UnpinnedOrdering {
                section: "empty_group",
            })?;

        // Channel/block header (native 46158..46225).
        let channel_mode = cfg_source.cfg_u32(0xa0)?;
        let quant_header = cfg_source.cfg_u32(0xc4)?;
        let header_flag = cfg_source.cfg_u32(0x118)?;
        {
            writer.write_bits(channel_mode, 2)?;
            writer.write_bits(quant_header.wrapping_sub(1), 5)?;
            writer.write_bits(header_flag, 1)?;
            Ok::<(), FrameAssemblyError>(())
        }?;

        let nblk = group.nblk;
        let quant_unit_count = cfg_source.cfg_u32(0xb0)? as usize;

        // IDWL section (native 46226..46260).
        for (i, obj) in group.objects.iter().enumerate().take(nblk) {
            let mode = obj.u32(0x1c70c)?;
            {
                writer.write_bits(mode, 2)?;
                Ok::<(), FrameAssemblyError>(())
            }?;
            pack_idwl(writer, group, i, obj)?;
        }

        // IDSF section (native 46261..46294).
        if quant_unit_count > 0 {
            for (i, obj) in group.objects.iter().enumerate().take(nblk) {
                let mode = obj.u32(0x1c73c)?;
                {
                    writer.write_bits(mode, 2)?;
                    Ok::<(), FrameAssemblyError>(())
                }?;
                pack_idsf(writer, group, i, obj)?;
            }
        }

        // IDCT section (native 46295..46377).
        let idct_quant_unit_count = syntax_group
            .map(|group| group.header().quant_unit_count)
            .unwrap_or(quant_unit_count);
        if idct_quant_unit_count > 0 {
            let bandwidth_gate = match syntax_group {
                Some(group) => u32::from(group.header().bandwidth_gate),
                None => cfg_source.cfg_u32(0x90)?,
            };
            {
                writer.write_bits(bandwidth_gate, 1)?;
                Ok::<(), FrameAssemblyError>(())
            }?;
            for (i, obj) in group.objects.iter().enumerate().take(nblk) {
                let typed = syntax_group.and_then(|group| group.channels().get(i));
                if syntax_group.is_some() && typed.is_none() {
                    return Err(FrameAssemblyError::UnpinnedOrdering {
                        section: "typed_channel_count",
                    });
                }
                let (bandwidth, mode) = match typed {
                    Some(channel) => (u32::from(channel.idct().bandwidth), channel.idct().mode),
                    None => (obj.u32(0x1074)?, obj.u32(0x1078)?),
                };
                {
                    writer.write_bits(bandwidth, 1)?;
                    writer.write_bits(mode, 2)?;
                    Ok::<(), FrameAssemblyError>(())
                }?;
                if let Some(channel) = typed {
                    pack_idct_syntax(writer, channel.idct())?;
                } else {
                    pack_idct(writer, group, i, obj, quant_unit_count)?;
                }
            }
        }

        // Spectral payload + per-block IDSPCQU tail (native 46378..46632). Each
        // block emits its descriptor-unit stream then its 4-bit level-word tail.
        let bandwidth_remap = cfg_source.cfg_u32(0x90)? == 0;
        for obj in group.objects.iter().take(nblk) {
            pack_spectral_block(writer, obj, quant_unit_count, bandwidth_remap)?;
            pack_spectral_idspcqu_block(writer, obj, cfg_source, quant_unit_count)?;
        }

        // Stereo config side data (native 46633..46781, gated `iVar19 == 2`).
        if nblk == 2 {
            pack_stereo_config_block(writer, cfg_source)?;
        }

        // Section 6: secondary-gain "gainB" flags (native 46786..46865). Per
        // block, `*(obj+8)+0x980` (1 bit), then `+0x984` (1 bit) and `+0x988+k*4`
        // (1 bit each, k in 0..`*(cfg+0xc8)`) when the previous flag is nonzero.
        let gainb_count = cfg_source.cfg_u32(0xc8)? as usize;
        for obj in group.objects.iter().take(nblk) {
            pack_gain_side_gainb(writer, obj, gainb_count)?;
        }

        // Section 7: gain NGC/IDLEV/IDLOC side data (native 46866..47053).
        for obj in group.objects.iter().take(nblk) {
            pack_gain_block(writer, group, obj)?;
        }

        // Section 8: GHA header (native 47055..47348), once per group, from the
        // first block's shared arena_root. `pack_gha_header` writes the 1-bit
        // `arena_root[0]` flag first (native 47055..47075); when that flag is 0 it
        // returns immediately after the single bit — the native `if (*piVar22 !=
        // 0)` gate (decompile 47077..~47565, skip target native 0x5751b) is FALSE,
        // so the rest of the GHA header AND the whole per-channel sections 9+10
        // loop are skipped. That is the GHA-disabled frame layout law
        // (`calc_nbits_for_gha_at5` absent arm == 1 bit, decompile 6813); first
        // ported this slice (docs/13 §5.1).
        let arena_flag0 = cfg_source.arena_u32(0)?;
        pack_gha_header(writer, cfg_source, nblk)?;

        // Sections 9 + 10: per-channel GHA side data then the per-wave payload
        // loop (native 47349..47568), gated on `arena_root[0] != 0` (the same
        // native `if (*piVar22 != 0)` gate). The idam gate is `arena_root[1] == 0`.
        if arena_flag0 != 0 {
            let arena_flag1 = cfg_source.arena_u32(1)?;
            for obj in group.objects.iter().take(nblk) {
                pack_gha_channel(writer, group, obj, arena_flag1)?;
            }
        }

        // Section D: post-payload gate (native 47570..47647). OUTSIDE the GHA
        // `arena_root[0]` gate (decompile 47566+): the 1-bit `cfg+0x94` gate and
        // its two 4-bit words pack even on GHA-absent frames.
        pack_post_payload(writer, cfg_source)?;
    }

    // Frame tail (native 47651..47713): 2-bit marker `3`, byte align, then `0x01`
    // stuffing to `frame_bytes`.
    {
        writer.write_frame_tail(state.frame_bytes * 8)?;
        Ok::<(), FrameAssemblyError>(())
    }?;

    Ok(())
}

fn pack_idwl(
    writer: &mut BitWriter<'_>,
    group: &BlockGroup,
    index: usize,
    obj: &ObjectState,
) -> Result<(), FrameAssemblyError> {
    let parity = obj.channel_index & 1;
    let mode = obj.u32(0x1c70c)?;
    let idx = dispatch_index(mode, parity);
    let config_count = obj.cfg_u32(0xc4)? as usize;
    let count = obj.u32(0x1c728)? as usize;

    match idx {
        // Native `pf_pack_idwl_table` (file offset 0xf3020) maps BOTH index 0
        // (ch0 mode-word 0) and index 4 (ch1 mode-word 0) to the same leaf
        // `pack_idwl_0_at5` (native `0x1fd40`). The pack-time dispatch site
        // (decompile 46251..46255) is unconditional after the 2-bit mode write;
        // the leaf has NO channel-dependent behavior (it never reads state[0]):
        // it writes `cfg[0xc4]` raw 3-bit word lengths from `0x1b5f8` — the same
        // array the 5/6 arms read as `current_values`. NATIVE-LIVE on every
        // syn_tone_997 frame, both channels (docs/12 §1.6(d), sweep of record
        // 2026-07-06 (n)).
        0 | 4 => {
            let values = obj.u32_array(0x1b5f8, config_count)?;
            pack_idwl_0_at5(writer, &values)?;
        }
        // Native `pf_pack_idwl_table` (file offset 0xf3020) index 1 =
        // `pack_idwl_1_at5` (native `0x1f540`; ch0 parity 0, mode-word 1). The
        // pack-time dispatch (decompile 46251..46255) is unconditional after the
        // 2-bit mode write. The ch0 tone-mode-1 prep tail (docs/12 §1.3) lays
        // down the aux triple `0x1c710/14/18`; the leaf reads them as
        // prefix_count / residual_bits / residual_base (native harness
        // `pack_idwl1_harness.json` word offsets 0x71c4/0x71c5/0x71c6). ch0 tone
        // mode 1 is LIVE on the dance-the-night sweep of record (7 blocks).
        1 => {
            // The leaf's value loops run over `0..count` (prefix `0..prefix_count`
            // then residual `prefix_count..count`), so read `count` plane words.
            let values = obj.u32_array(0x1c7f0, count)?;
            let fields = Idwl1Fields {
                channel_flag: obj.channel_index,
                selector_a: obj.u32(0x1c71c)?,
                selector_b: obj.u32(0x1c724)?,
                count,
                mode3_value: obj.u32(0x1c72c)?,
                prefix_count: obj.u32(0x1c710)? as usize,
                residual_bits: obj.u32(0x1c714)? as u8,
                residual_base: obj.u32(0x1c718)?,
                values: &values,
            };
            pack_idwl_1_at5(writer, &fields)?;
        }
        // Native `pf_pack_idwl_table` (file offset 0xf3020) index 2 =
        // `pack_idwl_2_at5` (native `0x1e9b0`; ch0 parity 0, mode-word 2). The
        // pack-time dispatch (decompile 46251..46255) is unconditional after the
        // 2-bit mode write. The prep tail (`serialize_idwl_object_range_b` +
        // `serialize_idwl_mode2_cfg_words`, decompile 44871-44901 / 45368-45398)
        // lays down the symbol plane at `0x1c870`, the field words `0x1c730`
        // (4-bit) / `0x1c734` (3-bit), the subgroup flag `0x1c738`, and the
        // shared-cfg group flags `cfg[0xd8..]` (count `cfg[0xd4]`). The leaf reads
        // symbols `0..count` (count word `0x1c728`) and `cfg[0xd4]` group flags,
        // matching `pack_idwl2_harness.json`. LIVE at 192 (sweat output frame
        // 5556, block 0; discovery_sweep_192_run.json).
        2 => {
            let group_count = obj.cfg_u32(0xd4)? as usize;
            let group_flags: Vec<u32> = (0..group_count)
                .map(|g| obj.cfg_u32(0xd8 + g * 4))
                .collect::<Result<_, _>>()?;
            let symbols = obj.u32_array(0x1c870, count)?;
            let fields = Idwl2Fields {
                channel_flag: obj.channel_index,
                selector_b: obj.u32(0x1c724)?,
                count,
                mode3_value: obj.u32(0x1c72c)?,
                subgroup_flag: obj.u32(0x1c738)?,
                huffman_selector: obj.u32(0x1c720)? as usize,
                field_3bits: obj.u32(0x1c734)?,
                field_4bits: obj.u32(0x1c730)?,
                group_flags: &group_flags,
                symbols: &symbols,
            };
            pack_idwl_2_at5(writer, &fields)?;
        }
        // Native `pf_pack_idwl_table` (file offset 0xf3020) maps BOTH index 3
        // (ch0 mode-word 3) and index 7 (ch1 mode-word 3) to the same leaf
        // `pack_idwl_3_at5` (native `0x1e190`). The pack-time dispatch site
        // (decompile 46251..46255) is unconditional after the 2-bit mode write;
        // there is no ch1-mode-3 special path at pack time. Channel-dependent
        // behavior is keyed inside the leaf off `channel_flag` (state[0] & 1),
        // not off the table index. docs/12 §1.2; ch1 mode-word 3 is LIVE on the
        // dance-the-night sweep of record (4 blocks select it).
        3 | 7 => {
            let values = obj.u32_array(0x1c7f0, config_count)?;
            let fields = Idwl3Fields {
                channel_flag: obj.channel_index,
                selector_a: obj.u32(0x1c71c)?,
                selector_b: obj.u32(0x1c724)?,
                count,
                config_count,
                mode3_value: obj.u32(0x1c72c)?,
                huffman_selector: obj.u32(0x1c720)? as usize,
                values: &values,
            };
            pack_idwl_3_at5(writer, &fields)?;
        }
        5 => {
            let previous = previous_object(group, obj)?;
            let current_values = obj.u32_array(0x1b5f8, config_count)?;
            let previous_values = previous.u32_array(0x1b5f8, config_count)?;
            let tail_flags = obj.u32_array(0x1c7f0, config_count)?;
            let fields = Idwl4Fields {
                channel_flag: obj.channel_index,
                selector_b: obj.u32(0x1c724)?,
                count,
                config_count,
                mode3_value: obj.u32(0x1c72c)?,
                huffman_selector: obj.u32(0x1c720)? as usize,
                current_values: &current_values,
                previous_values: &previous_values,
                tail_flags: &tail_flags,
            };
            pack_idwl_4_at5(writer, &fields)?;
        }
        6 => {
            let previous = previous_object(group, obj)?;
            let current_values = obj.u32_array(0x1b5f8, config_count)?;
            let previous_values = previous.u32_array(0x1b5f8, config_count)?;
            let tail_flags = obj.u32_array(0x1c7f0, config_count)?;
            let fields = Idwl4Fields {
                channel_flag: obj.channel_index,
                selector_b: obj.u32(0x1c724)?,
                count,
                config_count,
                mode3_value: obj.u32(0x1c72c)?,
                huffman_selector: obj.u32(0x1c720)? as usize,
                current_values: &current_values,
                previous_values: &previous_values,
                tail_flags: &tail_flags,
            };
            pack_idwl_5_at5(writer, &fields)?;
        }
        other => {
            return Err(FrameAssemblyError::UnsupportedDispatchIndex {
                family: "idwl",
                index: other,
            });
        }
    }
    let _ = index;
    Ok(())
}

fn pack_idsf(
    writer: &mut BitWriter<'_>,
    group: &BlockGroup,
    index: usize,
    obj: &ObjectState,
) -> Result<(), FrameAssemblyError> {
    let parity = obj.channel_index & 1;
    let mode = obj.u32(0x1c73c)?;
    let idx = dispatch_index(mode, parity);
    let count = obj.cfg_u32(0xb0)? as usize;

    match idx {
        // Native `pf_pack_idsf_table[0] == [4] == 0x1d330 == pack_idsf_0_at5`
        // (dispatch table at file offset 0xf3060 in libatrac.so.1.2.0). Both the
        // ch0 mode-word-0 index (0) and the ch1 mode-word-0 index (4) map to the
        // SAME parity-blind leaf: for i in 0..count it writes obj+0x1b678+i*4 as a
        // raw fixed-width 6-bit scalefactor field, reading no channel parity and
        // no other fields (decompile 15147..15195). The 2-bit mode prefix is
        // written by the dispatch caller (decompile 46289) before this leaf runs.
        // Live oracle: 12-34-am sweep output frame 297 block 0 (ch parity 0)
        // dispatches here with quant_unit_count 32 (> 0 — a real dispatch, not the
        // docs/12 (q) syn_silence quant_unit_count==0 memory-read artifact that
        // gates the whole IDSF section out); docs/12 §3.1. Index 4 has ZERO live
        // dispatch observations corpus-wide (all ch1 mode-0 rows carry
        // quant_unit_count==0 → artifact class), but is wired anyway because the
        // native table maps [4] to the same parity-blind leaf as [0] — this is
        // table+decompile evidence (evidence-hierarchy ranks 1-2), not
        // speculation. Rust ch1 costing may legitimately select mode 0 past the
        // accepted-drift parity horizon even where native did not, so the pack
        // surface must handle index 4.
        0 | 4 => {
            let values = obj.u32_array(0x1b678, count)?;
            pack_idsf_0_at5(writer, &values)?;
        }
        1 => {
            let mode_selector = (obj.u32(0x1c750)? & 3) as usize;
            let values = obj.i32_array(0x1c8f0 + mode_selector * 0x80, count)?;
            let fields = Idsf1Fields {
                mode_selector,
                field_0x1c758: obj.u32(0x1c758)?,
                field_0x1c754: obj.u32(0x1c754)?,
                prefix_count: obj.u32(0x1c740)? as usize,
                residual_bits: obj.u32(0x1c744)? as u8,
                residual_base: obj.u32(0x1c748)? as i32,
                count,
                values: &values,
            };
            pack_idsf_1_at5(writer, &fields)?;
        }
        2 => {
            let symbols = obj.u32_array(0x1ca70, count)?;
            let fields = Idsf2Fields {
                huffman_selector: (obj.u32(0x1c74c)? & 3) as usize,
                field_0x1c758: obj.u32(0x1c758)?,
                field_0x1c754: obj.u32(0x1c754)?,
                count,
                symbols: &symbols,
            };
            pack_idsf_2_at5(writer, &fields)?;
        }
        3 => {
            let mode_selector = (obj.u32(0x1c750)? & 3) as usize;
            let values = obj.i32_array(0x1c8f0 + mode_selector * 0x80, count)?;
            let fields = Idsf3Fields {
                mode_selector,
                huffman_selector: (obj.u32(0x1c74c)? & 3) as usize,
                field_0x1c758: obj.u32(0x1c758)?,
                field_0x1c754: obj.u32(0x1c754)?,
                count,
                values: &values,
            };
            pack_idsf_3_at5(writer, &fields)?;
        }
        5 => {
            let previous = previous_object(group, obj)?;
            let current_values = obj.u32_array(0x1b678, count)?;
            let previous_values = previous.u32_array(0x1b678, count)?;
            let fields = Idsf4Fields {
                huffman_selector: (obj.u32(0x1c74c)? & 3) as usize,
                count,
                current_values: &current_values,
                previous_values: &previous_values,
            };
            pack_idsf_4_at5(writer, &fields)?;
        }
        // Channel-1 mode 2 dispatches to native `pack_idsf_5_at5` (`0x1b7a0`),
        // the progressive previous-state SFC delta leaf.
        6 => {
            let previous = previous_object(group, obj)?;
            let current_values = obj.u32_array(0x1b678, count)?;
            let previous_values = previous.u32_array(0x1b678, count)?;
            let fields = Idsf4Fields {
                huffman_selector: (obj.u32(0x1c74c)? & 3) as usize,
                count,
                current_values: &current_values,
                previous_values: &previous_values,
            };
            pack_idsf_5_at5(writer, &fields)?;
        }
        // Channel-1 mode-word 3 dispatches to native `pf_pack_idsf_table[7] =
        // 0x13970 = pack_idsf_6_at5`, the disassembly-pinned pure no-op
        // (`push %ebp; mov %esp,%ebp; pop %ebp; ret`). The pack-time dispatch
        // (decompile 46289..46290) is unconditional after the 2-bit mode write;
        // there is no ch1-mode-3 special path. Semantics are
        // "unchanged-from-predecessor": the decoder counterpart
        // `unpack_idsf_6_at5` (0x13980) copies the previous object's `0x1b678`
        // scalefactor words, so all information is carried by the 2-bit mode
        // prefix alone (docs/12 §1.6; analogous to the §1.1 GHA IDSF selector-3
        // no-op). ch1 mode-word 3 is LIVE on the syn_tone_997 sweep (216 of 217
        // frames) and confirmed on the dance-the-night sweep of record.
        7 => pack_idsf_6_at5(writer)?,
        other => {
            return Err(FrameAssemblyError::UnsupportedDispatchIndex {
                family: "idsf",
                index: other,
            });
        }
    }
    let _ = index;
    Ok(())
}

fn pack_idct(
    writer: &mut BitWriter<'_>,
    group: &BlockGroup,
    index: usize,
    obj: &ObjectState,
    quant_unit_count: usize,
) -> Result<(), FrameAssemblyError> {
    let parity = obj.channel_index & 1;
    let mode = obj.u32(0x1078)?;
    let idx = dispatch_index(mode, parity);
    let bandwidth_mode = obj.cfg_u32(0x90)? as usize;

    let (count, active) = if obj.u32(0x1080)? == 0 {
        (
            Idct0Count::FullBandCount(quant_unit_count),
            quant_unit_count,
        )
    } else {
        let explicit = obj.u32(0x107c)? as usize;
        (Idct0Count::ExplicitCount(explicit), explicit)
    };

    let mut rows = Vec::with_capacity(active);
    for i in 0..active {
        rows.push(Idct0Row {
            mode: obj.u32(0x1084 + i * 4)?,
            value: obj.u32(0x1b578 + i * 4)?,
        });
    }

    match idx {
        0 | 4 => pack_idct_0_at5(writer, count, bandwidth_mode, &rows)?,
        1 | 5 => pack_idct_1_at5(writer, count, bandwidth_mode, &rows)?,
        2 | 6 => pack_idct_2_at5(writer, count, bandwidth_mode, &rows)?,
        // Index 3 (ch0 mode 3): the native leaf `pf_pack_idct_table[3]` =
        // `pack_idct_3_at5` (native 0x13350; Ghidra 0x23350, decompile
        // 8487-8493) is an EMPTY function — ch0 mode 3 packs ZERO payload
        // bits (the decoder's `unpack_idct_3_at5` zeroes the per-unit
        // values). First live natively at 48 kbps (dance-the-night output
        // frame 0, qu 1; oracle
        // calc_nbits mode-3 cost block was read-verified at docs/13 (pp).
        3 => {}
        7 => {
            let previous = previous_object(group, obj)?;
            let previous_values = previous.u32_array(0x1b578, active)?;
            pack_idct_4_at5(writer, count, bandwidth_mode, &rows, &previous_values)?;
        }
        other => {
            return Err(FrameAssemblyError::UnsupportedDispatchIndex {
                family: "idct",
                index: other,
            });
        }
    }
    let _ = index;
    Ok(())
}

fn pack_idct_syntax(
    writer: &mut BitWriter<'_>,
    syntax: &IdctSyntax,
) -> Result<(), FrameAssemblyError> {
    let count = match syntax.count {
        IdctCountSyntax::FullBand(count) => Idct0Count::FullBandCount(count),
        IdctCountSyntax::Explicit(count) => Idct0Count::ExplicitCount(count),
    };
    let rows = syntax
        .rows
        .iter()
        .map(|row| Idct0Row {
            mode: row.mode,
            value: row.value,
        })
        .collect::<Vec<_>>();

    match &syntax.encoding {
        IdctEncodingSyntax::Fixed => pack_idct_0_at5(writer, count, syntax.bandwidth_mode, &rows)?,
        IdctEncodingSyntax::Huffman => {
            pack_idct_1_at5(writer, count, syntax.bandwidth_mode, &rows)?
        }
        IdctEncodingSyntax::Delta => pack_idct_2_at5(writer, count, syntax.bandwidth_mode, &rows)?,
        IdctEncodingSyntax::Empty => {}
        IdctEncodingSyntax::Previous { values } => {
            pack_idct_4_at5(writer, count, syntax.bandwidth_mode, &rows, values)?
        }
    }
    Ok(())
}

/// Spectral descriptor-unit stream for one block (native 46380..46597).
///
/// For each active quant unit (`word_length > 0`) native derives the
/// `g_aaa_hcspec` descriptor slot from the object state — `iVar25 = *(obj+0x1074)`,
/// the selector `*(obj+0x1b578+qu*4)`, and the word length `*(obj+0x1b5f8+qu*4)` —
/// as byte offset `iVar25*0x540 + sel*0xa8 + wl*0x18`, i.e. slot index
/// `iVar25*56 + sel*7 + (wl-1)`. The grouped-symbol emission itself is the
/// trace-verified `pack_spectral_descriptor_unit` leaf.
fn pack_spectral_block(
    writer: &mut BitWriter<'_>,
    obj: &ObjectState,
    quant_unit_count: usize,
    bandwidth_remap: bool,
) -> Result<(), FrameAssemblyError> {
    if bandwidth_remap {
        // The `cfg+0x90 == 0` selector remap (`UNK_000c9c7c` matrix) is not
        // exercised by the 352 kbps profile (frame 0 has `cfg+0x90 == 1`); pin it
        // as unported rather than emit an unverified path.
        return Err(FrameAssemblyError::UnpinnedOrdering {
            section: "spectral_bandwidth_remap",
        });
    }

    let bandwidth_word = obj.u32(0x1074)? as usize;
    let nsps = nsps_at5();
    let isps = isps_at5();

    for qu in 0..quant_unit_count {
        let word_length = obj.u32(0x1b5f8 + qu * 4)? as i32;
        if word_length <= 0 {
            continue;
        }
        let word_length = word_length as usize;
        let selector = obj.u32(0x1b578 + qu * 4)? as usize;

        let slot_index = bandwidth_word * 56 + selector * 7 + (word_length - 1);
        let descriptor = SPECTRAL_DESCRIPTOR_SLOTS
            .get(slot_index)
            .ok_or(FrameAssemblyError::MissingSpectralSlot { slot_index })?;

        let sample_count = usize::from(nsps[qu]);
        let input_offset = 0x1b6f8 + usize::from(isps[qu]) * 2;
        let input = obj.u16_array(input_offset, sample_count)?;

        pack_spectral_descriptor_unit(writer, descriptor, &input, nsps[qu])?;
    }

    Ok(())
}

/// Look up the native `g_a_idspcqus_at5` tail count, honoring the past-end read
/// into the adjacent `g_a_idspcbands_at5` object (resolved via `readelf`; the
/// symbols are contiguous). Returns `None` for the `0xff` sentinel.
fn idspcqu_tail_count_at(index: usize) -> Option<usize> {
    let value = if index < G_A_IDSPCQUS_AT5.len() {
        G_A_IDSPCQUS_AT5[index]
    } else {
        *G_A_IDSPCBANDS_AT5.get(index - G_A_IDSPCQUS_AT5.len())?
    };
    (value != 0xff).then_some(usize::from(value) + 1)
}

/// Per-block IDSPCQU tail (native 46599..46628): when `quant_unit_count > 2` and
/// the `g_a_idspcqus_at5[cfg+0xc0 + 0x1f]` lookup is not the `0xff` sentinel,
/// emit `count+1` 4-bit level words from `*(obj+0x1c6f8+k*4)`.
fn pack_spectral_idspcqu_block(
    writer: &mut BitWriter<'_>,
    obj: &ObjectState,
    cfg_source: &ObjectState,
    quant_unit_count: usize,
) -> Result<(), FrameAssemblyError> {
    if quant_unit_count <= 2 {
        return Ok(());
    }
    let table_index = cfg_source.cfg_u32(0xc0)? as usize + 0x1f;
    let Some(count) = idspcqu_tail_count_at(table_index) else {
        return Ok(());
    };

    let level_words: Vec<u8> = obj
        .u32_array(0x1c6f8, count)?
        .into_iter()
        .map(|w| w as u8)
        .collect();
    pack_spectral_idspcqu_tail(writer, &level_words)?;
    Ok(())
}

/// Stereo config side data (native 46633..46781). Two independent 1-bit-gated
/// flag groups over the shared block config: the first from `cfg[0x48]`/`cfg[0x4c]`
/// plus `cfg[0x50 + k*4]`, the second from `cfg[0x00]`/`cfg[0x04]` plus
/// `cfg[0x08 + k*4]`, each inner run of length `cfg[0xc0]`.
fn pack_stereo_config_block(
    writer: &mut BitWriter<'_>,
    cfg_source: &ObjectState,
) -> Result<(), FrameAssemblyError> {
    let count = cfg_source.cfg_u32(0xc0)? as usize;

    let head = cfg_source.cfg_u32(0x48)?;
    writer.write_bits(head, 1)?;
    if head != 0 {
        let inner = cfg_source.cfg_u32(0x4c)?;
        writer.write_bits(inner, 1)?;
        if inner != 0 {
            for k in 0..count {
                writer.write_bits(cfg_source.cfg_u32(0x50 + k * 4)?, 1)?;
            }
        }
    }

    let head = cfg_source.cfg_u32(0x00)?;
    writer.write_bits(head, 1)?;
    if head != 0 {
        let inner = cfg_source.cfg_u32(0x04)?;
        writer.write_bits(inner, 1)?;
        if inner != 0 {
            for k in 0..count {
                writer.write_bits(cfg_source.cfg_u32(0x08 + k * 4)?, 1)?;
            }
        }
    }

    Ok(())
}

/// Native `sa_pmodebits_gh_{nwavs,idsf,idam}` (all identical, `.rodata` 0xc0bc0):
/// the prefix bit count for the per-channel GHA mode field, indexed by channel.
const GH_PMODEBITS: [u8; 2] = [1, 2];

/// Native `g_a_gh_nbands_pack` (`.rodata` 0xbc100, relocated from the
/// `g_hc_ghpc_nbands` descriptor at 0xf4e58): `(code, bit_length)` per entry.
/// The GHA header emits the `nbands` symbol from entry `nbands - 1`.
const G_A_GH_NBANDS_PACK: [(u16, u8); 16] = [
    (0x0000, 1),
    (0x0004, 3),
    (0x000a, 4),
    (0x000b, 4),
    (0x0018, 5),
    (0x0019, 5),
    (0x001a, 5),
    (0x001b, 5),
    (0x0038, 6),
    (0x0039, 6),
    (0x003a, 6),
    (0x003b, 6),
    (0x003c, 6),
    (0x003d, 6),
    (0x003e, 6),
    (0x003f, 6),
];

fn pmodebits(channel_index: u32) -> u8 {
    GH_PMODEBITS
        .get(channel_index as usize)
        .copied()
        .unwrap_or(2)
}

/// Section 6: secondary-gain "gainB" flags (native 46786..46865). Reads the
/// `*(obj+8)` gainB buffer: `+0x980` (1 bit); if nonzero `+0x984` (1 bit); if
/// that is nonzero, `+0x988+k*4` (1 bit each) for `k` in `0..count`.
fn pack_gain_side_gainb(
    writer: &mut BitWriter<'_>,
    obj: &ObjectState,
    count: usize,
) -> Result<(), FrameAssemblyError> {
    let flag0 = obj.gainb_u32(0x980)?;
    writer.write_bits(flag0, 1)?;
    if flag0 == 0 {
        return Ok(());
    }
    let flag1 = obj.gainb_u32(0x984)?;
    writer.write_bits(flag1, 1)?;
    if flag1 == 0 {
        return Ok(());
    }
    for k in 0..count {
        writer.write_bits(obj.gainb_u32(0x988 + k * 4)?, 1)?;
    }
    Ok(())
}

/// The native gain rows the gain leaves parse out of `*(obj+8)` (stride `0x98`,
/// count `*(obj+0x1b490)`): `+0x0` = gain-point count, `+0x4+k*4` = location[k],
/// `+0x20+k*4` = level[k].
struct GainRows {
    counts: Vec<u32>,
    locations: Vec<Vec<u32>>,
    levels_u32: Vec<Vec<u32>>,
    levels_i32: Vec<Vec<i32>>,
}

/// Parse `count` gain rows from `*(obj+8)`. Each row holds `+0x0` gain-point
/// count (clamped to the native 7-point maximum for the location/level arrays),
/// `+0x4+k*4` location[k], `+0x20+k*4` level[k]. The previous-state gain leaves
/// (`_4`/`_6`) read the *current* row count out of the previous object's raw
/// buffer, so `count` is passed explicitly rather than read per object.
fn parse_gain_rows(obj: &ObjectState, count: usize) -> Result<GainRows, FrameAssemblyError> {
    let mut rows = GainRows {
        counts: Vec::with_capacity(count),
        locations: Vec::with_capacity(count),
        levels_u32: Vec::with_capacity(count),
        levels_i32: Vec::with_capacity(count),
    };
    for r in 0..count {
        let base = r * 0x98;
        let n_raw = obj.gainb_u32(base)?;
        rows.counts.push(n_raw);
        let points = n_raw.min(7) as usize;
        let mut loc = Vec::with_capacity(points);
        let mut lvl_u = Vec::with_capacity(points);
        let mut lvl_i = Vec::with_capacity(points);
        for k in 0..points {
            loc.push(obj.gainb_u32(base + 0x4 + k * 4)?);
            let level = obj.gainb_u32(base + 0x20 + k * 4)?;
            lvl_u.push(level);
            lvl_i.push(level as i32);
        }
        rows.locations.push(loc);
        rows.levels_u32.push(lvl_u);
        rows.levels_i32.push(lvl_i);
    }
    Ok(rows)
}

/// Section 7: gain NGC/IDLEV/IDLOC side data for one block (native 46866..47053).
fn pack_gain_block(
    writer: &mut BitWriter<'_>,
    group: &BlockGroup,
    obj: &ObjectState,
) -> Result<(), FrameAssemblyError> {
    let present = obj.u32(0x1b484)?;
    let row_count_word = obj.u32(0x1b490)?;
    let flag = obj.u32(0x1b488)?;
    let gain_band_count = obj.u32(0x1b48c)?;
    {
        writer.write_bits(present, 1)?;
        if present != 0 {
            writer.write_bits(row_count_word.wrapping_sub(1), 4)?;
            writer.write_bits(flag, 1)?;
            if flag != 0 {
                writer.write_bits(gain_band_count.wrapping_sub(1), 4)?;
            }
        }
        Ok::<(), FrameAssemblyError>(())
    }?;
    if present == 0 {
        return Ok(());
    }

    let parity = obj.channel_index & 1;
    let row_count = row_count_word as usize;
    let rows = parse_gain_rows(obj, row_count)?;

    let level_rows: Vec<&[u32]> = rows.levels_u32.iter().map(Vec::as_slice).collect();
    let location_rows: Vec<&[u32]> = rows.locations.iter().map(Vec::as_slice).collect();

    let ngc_mode = obj.u32(0x1b494)?;
    let ngc_dispatch = dispatch_index(ngc_mode, parity);
    {
        writer.write_bits(ngc_mode, 2)?;
        match ngc_dispatch {
            0 | 4 => pack_gain_ngc_0_at5(writer, &rows.counts)?,
            1 | 5 => pack_gain_ngc_1_at5(writer, &rows.counts)?,
            2 => pack_gain_ngc_2_at5(writer, &rows.counts)?,
            3 => {
                let values: Vec<i32> = rows.counts.iter().map(|&value| value as i32).collect();
                let fields = Ngc3Fields {
                    bit_width: obj.u32(0x1b498)? as u8,
                    base: obj.u32(0x1b49c)? as i32,
                    values: &values,
                };
                pack_gain_ngc_3_at5(writer, &fields)?;
            }
            6 => {
                let prev = parse_gain_rows(previous_object(group, obj)?, row_count)?;
                pack_gain_ngc_4_at5(writer, &rows.counts, &prev.counts)?;
            }
            7 => pack_gain_ngc_5_at5(writer)?,
            other => {
                return Err(FrameAssemblyError::UnsupportedDispatchIndex {
                    family: "gain_ngc",
                    index: other,
                });
            }
        }
        Ok::<(), FrameAssemblyError>(())
    }?;

    let idlev_mode = obj.u32(0x1b4a0)?;
    let idlev_dispatch = dispatch_index(idlev_mode, parity);
    {
        writer.write_bits(idlev_mode, 2)?;
        match idlev_dispatch {
            0 | 4 => pack_gain_idlev_0_at5(writer, &level_rows)?,
            1 => pack_gain_idlev_1_at5(writer, &level_rows)?,
            2 => pack_gain_idlev_2_at5(writer, &level_rows)?,
            3 => {
                let level_rows_i32: Vec<&[i32]> =
                    rows.levels_i32.iter().map(Vec::as_slice).collect();
                let fields = Idlev3Fields {
                    bit_width: obj.u32(0x1b4a4)? as u8,
                    base: obj.u32(0x1b4a8)? as i32,
                    rows: &level_rows_i32,
                };
                pack_gain_idlev_3_at5(writer, &fields)?;
            }
            5 => {
                let prev = parse_gain_rows(previous_object(group, obj)?, row_count)?;
                let prv: Vec<&[u32]> = prev.levels_u32.iter().map(Vec::as_slice).collect();
                pack_gain_idlev_4_at5(writer, &level_rows, &prv)?;
            }
            6 => {
                let flags = obj.u32_array(0x1b4ac, rows.counts.len())?;
                pack_gain_idlev_5_at5(writer, &level_rows, &flags)?;
            }
            7 => pack_gain_idlev_6_at5(writer)?,
            other => {
                return Err(FrameAssemblyError::UnsupportedDispatchIndex {
                    family: "gain_idlev",
                    index: other,
                });
            }
        }
        Ok::<(), FrameAssemblyError>(())
    }?;

    let idloc_mode = obj.u32(0x1b4ec)?;
    let idloc_dispatch = dispatch_index(idloc_mode, parity);
    {
        writer.write_bits(idloc_mode, 2)?;
        match idloc_dispatch {
            0 | 4 => pack_gain_idloc_0_at5(writer, &location_rows)?,
            1 => {
                let idloc_rows = idloc_rows(&rows);
                pack_gain_idloc_1_at5(writer, &idloc_rows)?;
            }
            2 => {
                let idloc_rows = idloc_rows(&rows);
                pack_gain_idloc_2_at5(writer, &idloc_rows)?;
            }
            3 => {
                let fields = Idloc3Fields {
                    bit_width: obj.u32(0x1b4f0)? as u8,
                    base: obj.u32(0x1b4f4)? as i32,
                    rows: &location_rows,
                };
                pack_gain_idloc_3_at5(writer, &fields)?;
            }
            5 => {
                let prev = parse_gain_rows(previous_object(group, obj)?, row_count)?;
                let idloc_rows = idloc_rows(&rows);
                let prv: Vec<&[u32]> = prev.locations.iter().map(Vec::as_slice).collect();
                pack_gain_idloc_4_at5(writer, &idloc_rows, &prv)?;
            }
            6 => {
                let prev = parse_gain_rows(previous_object(group, obj)?, row_count)?;
                let prv: Vec<&[u32]> = prev.locations.iter().map(Vec::as_slice).collect();
                let idloc_rows = idloc_rows(&rows);
                let flags = obj.u32_array(0x1b4f8, rows.counts.len())?;
                pack_gain_idloc_5_at5(writer, &idloc_rows, &prv, &flags)?;
            }
            7 => {
                let prev = parse_gain_rows(previous_object(group, obj)?, row_count)?;
                let prv: Vec<&[u32]> = prev.locations.iter().map(Vec::as_slice).collect();
                let flags = obj.u32_array(0x1b538, rows.counts.len())?;
                pack_gain_idloc_6_at5(writer, &location_rows, &prv, &flags)?;
            }
            other => {
                return Err(FrameAssemblyError::UnsupportedDispatchIndex {
                    family: "gain_idloc",
                    index: other,
                });
            }
        }
        Ok::<(), FrameAssemblyError>(())
    }?;

    Ok(())
}

fn idloc_rows(rows: &GainRows) -> Vec<IdlocRow<'_>> {
    rows.locations
        .iter()
        .zip(rows.levels_i32.iter())
        .map(|(locations, levels)| IdlocRow { locations, levels })
        .collect()
}

/// Section 8: GHA header (native 47055..47348) from the shared arena_root.
fn pack_gha_header(
    writer: &mut BitWriter<'_>,
    obj: &ObjectState,
    nblk: usize,
) -> Result<(), FrameAssemblyError> {
    let flag0 = obj.arena_u32(0)?;
    writer.write_bits(flag0, 1)?;
    if flag0 == 0 {
        return Ok(());
    }
    writer.write_bits(obj.arena_u32(1)?, 1)?;

    let nbands = obj.arena_u32(2)? as usize;
    let (code, len) = G_A_GH_NBANDS_PACK
        .get(nbands.wrapping_sub(1))
        .copied()
        .ok_or(FrameAssemblyError::MissingNbandsSymbol { nbands })?;
    writer.write_bits(u32::from(code), len)?;

    if nblk == 2 {
        for (gate, subgate, base) in [(0xc4, 0xc5, 0xc6), (0xe8, 0xe9, 0xea), (0xd6, 0xd7, 0xd8)] {
            let g = obj.arena_u32(gate)?;
            writer.write_bits(g, 1)?;
            if g == 0 {
                continue;
            }
            let sg = obj.arena_u32(subgate)?;
            writer.write_bits(sg, 1)?;
            if sg == 0 {
                continue;
            }
            for k in 0..nbands {
                writer.write_bits(obj.arena_u32(base + k)?, 1)?;
            }
        }
    }

    Ok(())
}

/// Sections 9 + 10: per-channel GHA side data + per-wave payload for one block
/// (native 47349..47568).
fn pack_gha_channel(
    writer: &mut BitWriter<'_>,
    group: &BlockGroup,
    obj: &ObjectState,
    arena_flag1: u32,
) -> Result<(), FrameAssemblyError> {
    let channel = obj.channel_index;
    let nrec = obj.gha_records.len();
    let active: Vec<bool> = (0..nrec)
        .map(|r| Ok::<bool, FrameAssemblyError>(obj.u32(0x1c7b0 + r * 4)? != 0))
        .collect::<Result<_, _>>()?;

    // IDLOC: 1-bit gate only for channel 1, then dispatch.
    let idloc_mode = obj.u32(0x1c75c)?;
    {
        if channel == 1 {
            writer.write_bits(idloc_mode, 1)?;
        }
        match (idloc_mode & 1) as usize {
            0 => {
                let rows = gha_idloc_rows(obj, &active)?;
                pack_gh_idloc_0_at5(writer, &rows)?;
            }
            _ => pack_gh_idloc_1_at5(writer)?,
        }
        Ok::<(), FrameAssemblyError>(())
    }?;

    // NWAVS: the checkpoint sits before the prefix write.
    let nwavs_mode = obj.u32(0x1c760)?;
    let nb = pmodebits(channel);
    let nwavs_rows: Vec<GhNwavsRow> = (0..nrec)
        .map(|r| GhNwavsRow {
            active: active[r],
            value: obj.gha_records[r].len() as u32,
        })
        .collect();
    {
        if nb != 0 {
            writer.write_bits(nwavs_mode, nb)?;
        }
        match (nwavs_mode & 3) as usize {
            0 => pack_gh_nwavs_0_at5(writer, &nwavs_rows)?,
            1 => pack_gh_nwavs_1_at5(writer, &nwavs_rows)?,
            2 => {
                let previous = previous_object(group, obj)?;
                let previous_values: Vec<u32> = previous
                    .gha_records
                    .iter()
                    .map(|waves| waves.len() as u32)
                    .collect();
                pack_gh_nwavs_2_at5(writer, &nwavs_rows, &previous_values)?;
            }
            3 => pack_gh_nwavs_3_at5(writer)?,
            other => {
                return Err(FrameAssemblyError::UnsupportedDispatchIndex {
                    family: "gha_nwavs",
                    index: other,
                });
            }
        }
        Ok::<(), FrameAssemblyError>(())
    }?;

    // FREQ: 1-bit gate only for channel 1, then dispatch. The current rows (freq
    // per wave, `+0xc`) feed both the intra-block leaf (mode 0) and the
    // inter-channel differential leaf (mode 1).
    let freq_mode = obj.u32(0x1c764)?;
    let freq_value_rows: Vec<Vec<u32>> = obj
        .gha_records
        .iter()
        .map(|waves| waves.iter().map(|w| w.freq).collect())
        .collect();
    let freq_rows: Vec<GhFreqRow<'_>> = (0..nrec)
        .map(|r| GhFreqRow {
            active: active[r],
            values: &freq_value_rows[r],
        })
        .collect();
    {
        if channel == 1 {
            writer.write_bits(freq_mode, 1)?;
        }
        match (freq_mode & 1) as usize {
            0 => {
                let modes: Vec<u32> = (0..nrec)
                    .map(|r| obj.u32(0x1c770 + r * 4))
                    .collect::<Result<_, _>>()?;
                pack_gh_freq_0_at5(writer, &freq_rows, &modes)?;
            }
            _ => {
                let previous = previous_object(group, obj)?;
                let previous_value_rows: Vec<Vec<u32>> = previous
                    .gha_records
                    .iter()
                    .map(|waves| waves.iter().map(|w| w.freq).collect())
                    .collect();
                let previous_rows: Vec<&[u32]> =
                    previous_value_rows.iter().map(Vec::as_slice).collect();
                pack_gh_freq_1_at5(writer, &freq_rows, &previous_rows)?;
            }
        }
        Ok::<(), FrameAssemblyError>(())
    }?;

    // IDSF: prefix, then dispatch. header_mode = arena_root[1].
    let idsf_mode = obj.u32(0x1c768)?;
    let idsf_value_rows: Vec<Vec<u32>> = obj
        .gha_records
        .iter()
        .map(|waves| waves.iter().map(|w| w.idsf).collect())
        .collect();
    let idsf_rows: Vec<GhIdsfRow<'_>> = (0..nrec)
        .map(|r| GhIdsfRow {
            active: active[r],
            values: &idsf_value_rows[r],
        })
        .collect();
    {
        if nb != 0 {
            writer.write_bits(idsf_mode, nb)?;
        }
        match (idsf_mode & 3) as usize {
            0 => pack_gh_idsf_0_at5(writer, arena_flag1, &idsf_rows)?,
            1 => pack_gh_idsf_1_at5(writer, arena_flag1, &idsf_rows)?,
            // Mode 2 is the inter-channel differential IDSF leaf (native `0x1a840`):
            // idsf per wave (`+0x0`) deltaed against the previous object's
            // (`*(obj+0x28)`) record arena, indexed through the `*(obj+4)+0x11c`
            // predictor map (see `gha_idsf_predictor_indices`).
            2 => {
                let previous = previous_object(group, obj)?;
                let previous_value_rows: Vec<Vec<u32>> = previous
                    .gha_records
                    .iter()
                    .map(|waves| waves.iter().map(|w| w.idsf).collect())
                    .collect();
                let previous_rows: Vec<&[u32]> =
                    previous_value_rows.iter().map(Vec::as_slice).collect();
                let previous_index_rows = gha_idsf_predictor_indices(obj, &active)?;
                let previous_indices: Vec<&[i32]> =
                    previous_index_rows.iter().map(Vec::as_slice).collect();
                pack_gh_idsf_2_at5(
                    writer,
                    arena_flag1,
                    &idsf_rows,
                    &previous_rows,
                    &previous_indices,
                )?;
            }
            // Mode 3 is the "unchanged-from-predictor" no-op leaf (native
            // `pack_gh_idsf_3_at5`, `0x135b0`, a bare `return`): the IDSF section is
            // then exactly the nb-bit mode prefix and nothing else. The selection is
            // made by the `calc_nbits_for_gha_at5` costing (candidate 3, offered only
            // when the channel `has_previous`); native selects it only on channel 1.
            // Native-live per the dance-the-night sweep (45 output frames).
            3 => pack_gh_idsf_3_at5(writer)?,
            other => {
                return Err(FrameAssemblyError::UnsupportedDispatchIndex {
                    family: "gha_idsf",
                    index: other,
                });
            }
        }
        Ok::<(), FrameAssemblyError>(())
    }?;

    // IDAM: only when arena_root[1] == 0 (never for the 352 profile, arena[1]==1).
    if arena_flag1 == 0 {
        return Err(FrameAssemblyError::UnpinnedOrdering {
            section: "gha_idam",
        });
    }

    // Section 10: per-wave payload loop.
    {
        for r in 0..nrec {
            if !active[r] {
                continue;
            }
            for wave in &obj.gha_records[r] {
                writer.write_bits(wave.phase, 5)?;
            }
        }
        Ok::<(), FrameAssemblyError>(())
    }?;

    Ok(())
}

/// Build the per-record predictor-index slices for GHA IDSF mode 2 (native
/// `pack_gh_idsf_2_at5`, `0x1a840`). With `header_mode != 0` (arena_root[1],
/// always 1 at 352 kbps) the predictor map lives in the shared block config at
/// `*(obj+4)+0x11c` as i32 entries. The map base (`local_1c`) advances by each
/// ACTIVE record's current wave count (`gha_records[r].len()`) — inactive records
/// neither consume entries nor advance the base — and each current wave `w` in an
/// active record reads `map[base + w]`. Inactive records get an empty slice (the
/// leaf skips them). A `-1` (`0xffffffff`) entry is the no-predictor sentinel the
/// leaf resolves against the raw-symbol base.
fn gha_idsf_predictor_indices(
    obj: &ObjectState,
    active: &[bool],
) -> Result<Vec<Vec<i32>>, FrameAssemblyError> {
    let mut rows: Vec<Vec<i32>> = Vec::with_capacity(obj.gha_records.len());
    let mut base = 0usize;
    for (r, waves) in obj.gha_records.iter().enumerate() {
        if !active[r] {
            rows.push(Vec::new());
            continue;
        }
        let mut slice = Vec::with_capacity(waves.len());
        for w in 0..waves.len() {
            slice.push(obj.cfg_u32(0x11c + (base + w) * 4)? as i32);
        }
        base += waves.len();
        rows.push(slice);
    }
    Ok(rows)
}

fn gha_idloc_rows(
    obj: &ObjectState,
    active: &[bool],
) -> Result<Vec<GhIdlocRow>, FrameAssemblyError> {
    (0..obj.gha_records.len())
        .map(|r| {
            let word = r * 10;
            Ok(GhIdlocRow {
                active: active[r],
                first_flag: obj.p1_u32(word + 5)?,
                first_location: obj.p1_u32(word + 7)?,
                second_flag: obj.p1_u32(word + 6)?,
                second_location: obj.p1_u32(word + 8)?,
            })
        })
        .collect()
}

/// Section D: post-payload gate (native 47570..47647).
fn pack_post_payload(
    writer: &mut BitWriter<'_>,
    obj: &ObjectState,
) -> Result<(), FrameAssemblyError> {
    let flag = obj.cfg_u32(0x94)?;
    writer.write_bits(flag, 1)?;
    if flag == 1 {
        writer.write_bits(obj.cfg_u32(0x98)?, 4)?;
        writer.write_bits(obj.cfg_u32(0x9c)?, 4)?;
    }
    Ok(())
}

fn previous_object<'a>(
    group: &'a BlockGroup,
    obj: &ObjectState,
) -> Result<&'a ObjectState, FrameAssemblyError> {
    let index = obj
        .previous_index
        .ok_or(FrameAssemblyError::MissingPreviousObject { block_index: 0 })?;
    group
        .objects
        .get(index)
        .ok_or(FrameAssemblyError::MissingPreviousObject { block_index: index })
}
