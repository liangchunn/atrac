#![cfg_attr(not(test), allow(dead_code))]

use crate::dsp::dba::{
    DBA_GAIN_INFO_EXT_PREFIX, DBA_GAIN_INFO_STRIDE, DbaAt3DataResult, DbaToneTable,
    dba_magic_round_bits,
};
use crate::dsp::pack::pack_store_from_msb;
use crate::tables::dba;

pub(crate) struct DbaPackChannel<'a> {
    pub(crate) data: &'a DbaAt3DataResult,
    pub(crate) gain_side_info_ext: &'a [i32],
    pub(crate) channel_mode: i32,
    pub(crate) channel_flags: i32,
}

pub(crate) struct DbaPackFrame<'a> {
    pub(crate) channels: &'a [DbaPackChannel<'a>],
    pub(crate) channel_bytes: usize,
    pub(crate) js_enabled: bool,
    pub(crate) chconv_abs_modes: [i32; 4],
}

fn hcspec_code_len(entry: u32) -> (u32, u32) {
    let len = entry >> 16;
    let code = entry & 0xffff;
    if len == 0 {
        (0, 0)
    } else {
        (code >> (16 - len), len)
    }
}

fn write_hcspec(entry: u32, buffer: &mut [u8], bit_pos: &mut u32) {
    let (code, len) = hcspec_code_len(entry);
    pack_store_from_msb(code, len, buffer, bit_pos);
}

fn dba_quant_scale(presence: i32, idsf: i32) -> f32 {
    let phase = (idsf as u32).wrapping_mul(0x002b_0000);
    let table_index = (((phase >> 23) as i32 + presence) * 3 - idsf) as usize;
    f32::from_bits(dba::DBA_SCALE_LOOKUP[table_index].wrapping_sub(phase & 0x7f80_0000))
}

fn dba_quantized(sample: f32, scale: f32) -> u32 {
    dba_magic_round_bits(sample, scale)
}

fn dba_pack_gain(
    channel: &DbaPackChannel<'_>,
    ntones: usize,
    buffer: &mut [u8],
    bit_pos: &mut u32,
) {
    for band in 0..ntones {
        let base = DBA_GAIN_INFO_EXT_PREFIX + band * DBA_GAIN_INFO_STRIDE;
        let count = channel
            .gain_side_info_ext
            .get(base + 0x2a)
            .copied()
            .unwrap_or_default()
            .clamp(0, 7);
        pack_store_from_msb(count as u32, 3, buffer, bit_pos);
        for event in 0..count as usize {
            let level = channel
                .gain_side_info_ext
                .get(base + event)
                .copied()
                .unwrap_or(4);
            let loc = channel
                .gain_side_info_ext
                .get(base - 8 + event)
                .copied()
                .unwrap_or_default();
            pack_store_from_msb((level * 0x20 + loc) as u32, 9, buffer, bit_pos);
        }
    }
}

fn dba_pack_tones(
    table: &DbaToneTable,
    tone_mode: usize,
    ntones: usize,
    coding_layout: i32,
    buffer: &mut [u8],
    bit_pos: &mut u32,
) -> Result<(), i32> {
    for bank_idx in 0..tone_mode {
        let Some(bank) = table.banks.get(bank_idx) else {
            return Err(-1);
        };
        let mut active = bank.active_quarters[0];
        for idx in 1..ntones {
            active = bank.active_quarters[idx] + active * 2;
        }
        pack_store_from_msb(active as u32, ntones as u32, buffer, bit_pos);
        pack_store_from_msb((bank.idwl + bank.width * 8) as u32, 6, buffer, bit_pos);

        let huff = dba::dba_hcspec_packed_table(bank.idwl, coding_layout);
        if huff.is_empty() {
            return Err(-1);
        }
        for group_idx in 0..ntones * 4 {
            if bank.active_quarters[group_idx >> 2] == 0 {
                continue;
            }
            let group = &bank.groups[group_idx];
            pack_store_from_msb(group.len() as u32, 3, buffer, bit_pos);
            for &slot in group {
                let Some(component) = table.components.get(slot) else {
                    return Err(-1);
                };
                pack_store_from_msb(component.idsf as u32, 6, buffer, bit_pos);
                pack_store_from_msb((component.position as u32) & 0x3f, 6, buffer, bit_pos);
                for idx in 0..=bank.width as usize {
                    let value = component.quantized[idx] as usize;
                    let Some(&entry) = huff.get(value) else {
                        return Err(-1);
                    };
                    write_hcspec(entry, buffer, bit_pos);
                }
            }
        }
    }
    Ok(())
}

fn dba_pack_spectrum(
    data: &DbaAt3DataResult,
    buffer: &mut [u8],
    bit_pos: &mut u32,
) -> Result<(), i32> {
    pack_store_from_msb((data.nunits * 2 - 2) as u32, 6, buffer, bit_pos);
    for band in 0..data.nunits as usize {
        pack_store_from_msb(data.presence[band] as u32, 3, buffer, bit_pos);
    }
    for band in 0..data.nunits as usize {
        if data.presence[band] != 0 {
            pack_store_from_msb(data.allocations[band] as u32, 6, buffer, bit_pos);
        }
    }

    for band in 0..data.nunits as usize {
        let presence = data.presence[band];
        if presence == 0 {
            continue;
        }
        let idsf = data.allocations[band];
        let scale = dba_quant_scale(presence, idsf);
        let start = dba::DBA_QTSTART[band] as usize;
        let end = dba::DBA_QTEND[band] as usize;
        if presence == 1 {
            let huff = &dba::DBA_HCSPEC01;
            for chunk in data.residual_spectrum[start..end].chunks_exact(8) {
                let pair0 = ((dba_quantized(chunk[0], scale) & 3) << 2)
                    | (dba_quantized(chunk[1], scale) & 3);
                let pair1 = ((dba_quantized(chunk[2], scale) & 3) << 2)
                    | (dba_quantized(chunk[3], scale) & 3);
                let pair2 = ((dba_quantized(chunk[4], scale) & 3) << 2)
                    | (dba_quantized(chunk[5], scale) & 3);
                let pair3 = ((dba_quantized(chunk[6], scale) & 3) << 2)
                    | (dba_quantized(chunk[7], scale) & 3);
                write_hcspec(huff[pair0 as usize], buffer, bit_pos);
                write_hcspec(huff[pair1 as usize], buffer, bit_pos);
                write_hcspec(huff[pair2 as usize], buffer, bit_pos);
                write_hcspec(huff[pair3 as usize], buffer, bit_pos);
            }
        } else {
            let huff = dba::dba_hcspec_table(presence);
            let Some(&mask) = dba::DBA_HUF_MASK.get((presence - 2) as usize) else {
                return Err(-1);
            };
            if huff.is_empty() {
                return Err(-1);
            }
            for &sample in &data.residual_spectrum[start..end] {
                let value = (dba_quantized(sample, scale) & mask) as usize;
                let Some(&entry) = huff.get(value) else {
                    return Err(-1);
                };
                write_hcspec(entry, buffer, bit_pos);
            }
        }
    }
    Ok(())
}

pub(crate) fn dba_pack_channel(
    channel: &DbaPackChannel<'_>,
    chconv_abs_modes: [i32; 4],
    buffer: &mut [u8],
    byte_offset: usize,
) -> Result<usize, i32> {
    let ntones = channel.data.ntones.max(0) as usize;
    let tone_mode = channel.data.tone_mode.max(0) as usize;
    let mut header_offset = byte_offset;
    if channel.channel_mode != 0 {
        let marker = chconv_abs_modes[1] + (chconv_abs_modes[0] + channel.channel_flags * 4) * 4;
        if let Some(slot) = buffer.get_mut(byte_offset) {
            *slot = marker as u8;
        }
        header_offset += 1;
        let header =
            (chconv_abs_modes[3] + chconv_abs_modes[2] * 4) * 0x10 + 0x0c + channel.data.ntones - 1;
        if let Some(slot) = buffer.get_mut(header_offset) {
            *slot = header as u8;
        }
    } else if let Some(slot) = buffer.get_mut(byte_offset) {
        *slot = (-0x60 + channel.data.ntones - 1) as u8;
    }

    let mut bit_pos = (header_offset as u32) * 8 + 8;
    dba_pack_gain(channel, ntones, buffer, &mut bit_pos);
    pack_store_from_msb(tone_mode as u32, 5, buffer, &mut bit_pos);
    if tone_mode != 0 {
        pack_store_from_msb(channel.data.coding_layout as u32, 2, buffer, &mut bit_pos);
        dba_pack_tones(
            &channel.data.tone_table,
            tone_mode,
            ntones,
            channel.data.coding_layout,
            buffer,
            &mut bit_pos,
        )?;
    }
    dba_pack_spectrum(channel.data, buffer, &mut bit_pos)?;
    Ok(((bit_pos + 7) >> 3) as usize)
}

pub(crate) fn dba_pack_frame(frame: DbaPackFrame<'_>, buffer: &mut [u8]) -> Result<(), i32> {
    buffer.fill(0);
    let mut byte_offset = 0usize;
    for (channel_idx, channel) in frame.channels.iter().enumerate() {
        let start = byte_offset;
        let end = dba_pack_channel(channel, frame.chconv_abs_modes, buffer, start)?;
        byte_offset += frame.channel_bytes;
        if frame.js_enabled {
            if channel_idx == 0 {
                byte_offset = end;
            } else {
                let reverse_start = byte_offset.saturating_sub(frame.channel_bytes);
                let reverse_end = (frame.channel_bytes * 2).min(buffer.len());
                buffer[reverse_start..reverse_end].reverse();
            }
        }
    }
    Ok(())
}
