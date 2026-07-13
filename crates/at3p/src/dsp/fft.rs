#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FftError {
    UnsupportedCftmdlShape { count: usize, step: usize },
    UnsupportedDftXCount { count: usize },
    UnsupportedDftVShape { count: usize, stride: usize },
    UnsupportedRdftvCount { count: usize },
    DataTooShort { needed: usize, actual: usize },
    OutputTooShort { needed: usize, actual: usize },
    PermutationTableTooShort { needed: usize, actual: usize },
    TableTooShort { needed: usize, actual: usize },
}

pub fn cftmdl_at5(
    data: &mut [f32],
    table: &[f32],
    count: usize,
    step: usize,
) -> Result<(), FftError> {
    if count != 256 || (step != 8 && step != 32) {
        return Err(FftError::UnsupportedCftmdlShape { count, step });
    }
    if data.len() < count {
        return Err(FftError::DataTooShort {
            needed: count,
            actual: data.len(),
        });
    }
    if table.len() < 128 {
        return Err(FftError::TableTooShort {
            needed: 128,
            actual: table.len(),
        });
    }

    let data = &mut data[..count];
    cftmdl_stage(data, table, count, step);
    Ok(())
}

fn cftmdl_stage(data: &mut [f32], table: &[f32], count: usize, step: usize) {
    let mut index = 0;
    while index < step {
        let idx1 = step + index;
        let idx2 = step + idx1;
        let idx3 = step + idx2;

        let f1 = data[index];
        let f2 = data[idx1];
        let f7 = f1 + f2;
        let f3 = data[index + 1];
        let f4 = data[idx1 + 1];
        let f5 = data[idx3];
        let f9 = f3 + f4;
        let f3 = f3 - f4;
        let f8 = data[idx2] + f5;
        let f1 = f1 - f2;
        let f2 = data[idx2 + 1];
        let f4 = data[idx3 + 1];
        let f6 = f2 + f4;
        let f2 = f2 - f4;
        let f4 = data[idx2] - f5;

        data[index] = f7 + f8;
        data[index + 1] = f9 + f6;
        data[idx2] = f7 - f8;
        data[idx2 + 1] = f9 - f6;
        data[idx1] = f1 - f2;
        data[idx1 + 1] = f3 + f4;
        data[idx3] = f1 + f2;
        data[idx3 + 1] = f3 - f4;

        index += 2;
    }

    let scale = table[2];
    index = step * 4;
    while index < step * 5 {
        let idx1 = step + index;
        let idx2 = step + idx1;
        let idx3 = step + idx2;

        let f2 = data[index];
        let f3 = data[idx1];
        let f8 = f2 + f3;
        let f4 = data[index + 1];
        let f2 = f2 - f3;
        let f3 = data[idx1 + 1];
        let f9 = f4 - f3;
        let f5 = data[idx3];
        let f6 = data[idx2];
        let f7 = data[idx2 + 1];
        let f11 = f5 + f6;
        let f4 = f4 + f3;
        let f3 = data[idx3 + 1];
        let f10 = f7 + f3;
        let f7 = f7 - f3;
        let f6 = f6 - f5;

        data[index] = f8 + f11;
        data[index + 1] = f4 + f10;
        data[idx2] = f10 - f4;
        let f3 = f2 - f7;
        let f4 = f9 + f6;
        data[idx2 + 1] = f8 - f11;
        let f6 = f6 - f9;
        let f7 = f7 + f2;
        data[idx1] = (f3 - f4) * scale;
        data[idx1 + 1] = (f3 + f4) * scale;
        data[idx3] = (f6 - f7) * scale;
        data[idx3 + 1] = (f6 + f7) * scale;

        index += 2;
    }

    let mut local = 0;
    index = step * 8;
    while index < count {
        local += 2;
        let f1 = table[local + 1];
        let f2 = table[local];
        let f3 = table[local * 2 + 1];
        let f4 = table[local * 2];
        let f6 = f4 - f3 * (f1 + f1);
        let f5 = (f1 + f1) * f4 - f3;

        let mut inner = index;
        while inner < step + index {
            let idx1 = step + inner;
            let idx2 = step + idx1;
            let idx3 = step + idx2;

            let f7 = data[inner];
            let f8 = data[idx1 + 1];
            let f12 = data[inner + 1] + f8;
            let f8 = data[inner + 1] - f8;
            let f9 = data[idx1];
            let f14 = f7 - f9;
            let f10 = data[idx3 + 1];
            let f11 = data[idx2];
            let f16 = data[idx2 + 1] + f10;
            let f10 = data[idx2 + 1] - f10;
            let f7 = f7 + f9;
            let f9 = data[idx3];
            let f15 = f11 + f9;
            let f13 = f7 - f15;
            let f11 = f11 - f9;

            data[inner] = f7 + f15;
            let f7 = f12 - f16;
            data[inner + 1] = f16 + f12;
            let f9 = f14 - f10;
            data[idx2] = f2 * f13 - f1 * f7;
            data[idx2 + 1] = f7 * f2 + f13 * f1;
            let f7 = f8 + f11;
            data[idx1] = f4 * f9 - f3 * f7;
            let f14 = f14 + f10;
            data[idx1 + 1] = f7 * f4 + f9 * f3;
            let f8 = f8 - f11;
            data[idx3] = f6 * f14 - f5 * f8;
            data[idx3 + 1] = f5 * f14 + f6 * f8;

            inner += 2;
        }

        let f3 = table[local * 2 + 2];
        let f4 = table[local * 2 + 3];
        let f6 = f3 - f4 * (f2 + f2);
        let f5 = (f2 + f2) * f3 - f4;
        let mut idx0 = index + step * 4;
        let idx_end = step + idx0;
        let mut idx1 = idx_end;
        while idx0 < idx_end {
            let f7 = data[idx1 + 1];
            let f12 = data[idx0 + 1] + f7;
            let f7 = data[idx0 + 1] - f7;
            let f8 = data[idx1];
            let idx2 = step + idx1;
            let idx3 = step + idx2;
            let f13 = data[idx0] + f8;
            let f8 = data[idx0] - f8;
            let f9 = data[idx3 + 1];
            let f10 = data[idx2];
            let f16 = data[idx2 + 1] + f9;
            let f9 = data[idx2 + 1] - f9;
            let f11 = data[idx3];
            let f15 = f10 + f11;
            let f14 = f13 - f15;
            let f10 = f10 - f11;

            data[idx0] = f13 + f15;
            let f13 = f12 - f16;
            data[idx0 + 1] = f12 + f16;
            let f12 = f8 - f9;
            data[idx2 + 1] = -f1 * f13 + f14 * f2;
            let f11 = f7 + f10;
            data[idx2] = -f1 * f14 - f2 * f13;
            data[idx1] = f3 * f12 - f4 * f11;
            let f7 = f7 - f10;
            let f8 = f8 + f9;
            data[idx1 + 1] = f12 * f4 + f11 * f3;
            data[idx3] = f6 * f8 - f5 * f7;
            data[idx3 + 1] = f6 * f7 + f5 * f8;

            idx0 += 2;
            idx1 = step + idx0;
        }

        index += step * 8;
    }
}

pub fn rdftv_at5(
    data: &mut [f32],
    ip_table: &[u32],
    sc_table: &[f32],
    count: usize,
) -> Result<(), FftError> {
    let ip_needed = rdftv_ip_entries(count)?;
    let sc_needed = count / 2;
    if data.len() < count {
        return Err(FftError::DataTooShort {
            needed: count,
            actual: data.len(),
        });
    }
    if ip_table.len() < ip_needed {
        return Err(FftError::PermutationTableTooShort {
            needed: ip_needed,
            actual: ip_table.len(),
        });
    }
    if sc_table.len() < sc_needed {
        return Err(FftError::TableTooShort {
            needed: sc_needed,
            actual: sc_table.len(),
        });
    }

    let data = &mut data[..count];
    rdftv_permute(data, &ip_table[..ip_needed], count);
    rdftv_kernel(data, &sc_table[..sc_needed], count);
    Ok(())
}

pub fn dft_v_at5(
    input: &[f32],
    stride: usize,
    count: usize,
    output: &mut [f32],
    ip_table: &[u32],
    sc_table: &[f32],
) -> Result<(), FftError> {
    // Native dft_v_at5 (0x45d10) gathers `count` samples with `stride` and
    // selects tables by count (16/32/64/128/256).
    if !matches!(count, 16 | 32 | 64 | 128 | 256) || stride == 0 {
        return Err(FftError::UnsupportedDftVShape { count, stride });
    }
    let input_needed = (count - 1) * stride + 1;
    if input.len() < input_needed {
        return Err(FftError::DataTooShort {
            needed: input_needed,
            actual: input.len(),
        });
    }
    let output_needed = count / 2 + 1;
    if output.len() < output_needed {
        return Err(FftError::OutputTooShort {
            needed: output_needed,
            actual: output.len(),
        });
    }

    let mut data = [0.0f32; 256];
    for index in 0..count {
        data[index] = input[index * stride];
    }

    rdftv_at5(&mut data[..count], ip_table, sc_table, count)?;
    let nyquist = data[1];
    data[1] = 0.0;

    for bin in 0..count / 2 {
        let real = data[bin * 2];
        let imag = data[bin * 2 + 1];
        output[bin] = (real * real + imag * imag).sqrt();
    }
    output[count / 2] = nyquist.abs();

    Ok(())
}

pub fn dft_x_at5(
    input: &[f32],
    count: usize,
    output: &mut [f32],
    ip_table: &[u32],
    sc_table: &[f32],
) -> Result<(), FftError> {
    if count > 256 {
        return Err(FftError::UnsupportedDftXCount { count });
    }
    if input.len() < count {
        return Err(FftError::DataTooShort {
            needed: count,
            actual: input.len(),
        });
    }
    if output.len() < 129 {
        return Err(FftError::OutputTooShort {
            needed: 129,
            actual: output.len(),
        });
    }

    let mut data = [0.0f32; 256];
    data[..count].copy_from_slice(&input[..count]);

    rdftv_at5(&mut data, ip_table, sc_table, 256)?;
    let nyquist = data[1];
    data[1] = 0.0;

    for bin in 0..128 {
        let real = data[bin * 2];
        let imag = data[bin * 2 + 1];
        output[bin] = real * real + imag * imag;
    }
    output[128] = nyquist * nyquist;

    Ok(())
}

fn rdftv_ip_entries(count: usize) -> Result<usize, FftError> {
    match count {
        16 | 32 => Ok(2),
        64 | 128 => Ok(4),
        256 => Ok(8),
        _ => Err(FftError::UnsupportedRdftvCount { count }),
    }
}

fn rdftv_permute(data: &mut [f32], ip_table: &[u32], count: usize) {
    let mut groups = 1usize;
    let mut reduced_count = count;
    if count > 8 {
        loop {
            reduced_count >>= 1;
            let previous_span = groups << 4;
            groups *= 2;
            if previous_span >= reduced_count {
                break;
            }
        }
    }

    let group_stride = groups * 2;
    if groups << 3 == reduced_count {
        let mut group_index = 0usize;
        let mut diagonal_offset = group_stride;
        while group_index < groups {
            let mut table_value = ip_table[group_index] as usize;
            if group_index > 0 {
                let mut inner = 0usize;
                while inner < group_index {
                    let mut left = table_value + inner * 2;
                    let mut right = group_index * 2 + ip_table[inner] as usize;
                    swap_complex_pair(data, left, right);

                    left += group_stride;
                    right += groups * 4;
                    swap_complex_pair(data, left, right);

                    left += group_stride;
                    right -= groups * 2;
                    swap_complex_pair(data, left, right);

                    left += group_stride;
                    right += groups * 4;
                    swap_complex_pair(data, left, right);

                    inner += 1;
                }
            }

            table_value += diagonal_offset;
            swap_complex_pair(data, table_value, table_value + group_stride);

            group_index += 1;
            diagonal_offset += 2;
        }
    } else {
        let mut group_index = 1usize;
        while group_index < groups {
            let table_value = ip_table[group_index] as usize;
            let mut inner = 0usize;
            while inner < group_index {
                let left = table_value + inner * 2;
                let right = group_index * 2 + ip_table[inner] as usize;
                swap_complex_pair(data, left, right);
                swap_complex_pair(data, left + group_stride, right + group_stride);
                inner += 1;
            }
            group_index += 1;
        }
    }
}

fn swap_complex_pair(data: &mut [f32], left: usize, right: usize) {
    let left_real = data[left];
    let left_imag = data[left + 1];
    let right_real = data[right];
    let right_imag = data[right + 1];
    data[left] = right_real;
    data[left + 1] = right_imag;
    data[right] = left_real;
    data[right + 1] = left_imag;
}

fn rdftv_kernel(data: &mut [f32], table: &[f32], count: usize) {
    rdftv_base16(data, table);
    rdftv_16_point_blocks(data, table, count);
    let final_span = rdftv_staged_butterflies(data, table, count);
    rdftv_final_butterfly(data, count, final_span);
    rdftv_real_postprocess(data, table, count);
}

fn rdftv_base16(data: &mut [f32], table: &[f32]) {
    let f2 = data[0] + data[2];
    let f7 = data[1] + data[3];
    let f6 = data[1] - data[3];
    let f8 = data[4] + data[6];
    let f5 = data[5] + data[7];
    let f4 = data[5] - data[7];
    let f3 = data[0] - data[2];
    let f9 = data[4] - data[6];
    data[0] = f2 + f8;
    data[1] = f7 + f5;
    data[4] = f2 - f8;
    data[5] = f7 - f5;
    data[2] = f3 - f4;
    data[3] = f6 + f9;
    data[6] = f4 + f3;
    data[7] = f6 - f9;

    let f8 = data[8] + data[10];
    let scale = table[2];
    let f4 = data[9] + data[11];
    let f9 = data[9] - data[11];
    let f3 = data[8] - data[10];
    let f10 = data[14] + data[12];
    let f5 = data[13] + data[15];
    let f7 = data[13] - data[15];
    let f6 = data[12] - data[14];
    data[8] = f8 + f10;
    data[9] = f4 + f5;
    data[12] = f5 - f4;
    let f5 = f9 + f6;
    let f4 = f3 - f7;
    data[13] = f8 - f10;
    let f6 = f6 - f9;
    let f7 = f7 + f3;
    data[10] = (f4 - f5) * scale;
    data[11] = (f4 + f5) * scale;
    data[14] = (f6 - f7) * scale;
    data[15] = (f6 + f7) * scale;
}

fn rdftv_16_point_blocks(data: &mut [f32], table: &[f32], count: usize) {
    if count <= 0x10 {
        return;
    }

    let mut base = 0usize;
    let mut table_index = 0usize;
    let mut cursor = 0x10usize;
    loop {
        base += 0x10;
        table_index += 2;
        cursor += 0x10;

        let f2 = table[table_index + 1];
        let f3 = table[table_index];
        let f4 = table[table_index * 2 + 1];
        let f5 = table[table_index * 2];
        let f10 = f5 - f4 * (f2 + f2);
        let f9 = (f2 + f2) * f5 - f4;

        let x0 = data[base];
        let x1 = data[base + 1];
        let x2 = data[base + 2];
        let x3 = data[base + 3];
        let x4 = data[base + 4];
        let x5 = data[base + 5];
        let x6 = data[base + 6];
        let x7 = data[base + 7];
        let f11 = x1 + x3;
        let f12 = x1 - x3;
        let f13 = x5 + x7;
        let f15 = x5 - x7;
        data[base] = x0 + x2 + x4 + x6;
        let f14 = f11 - f13;
        data[base + 1] = f11 + f13;
        let f6 = (x0 + x2) - (x4 + x6);
        let f11 = (x0 - x2) + f15;
        let f13 = f12 - (x4 - x6);
        data[base + 4] = f3 * f6 - f2 * f14;
        let f15 = (x0 - x2) - f15;
        data[base + 5] = f3 * f14 + f2 * f6;
        let f12 = f12 + (x4 - x6);
        data[base + 2] = f5 * f15 - f4 * f12;
        data[base + 3] = f4 * f15 + f5 * f12;
        data[base + 6] = f10 * f11 - f9 * f13;
        data[base + 7] = f13 * f10 + f11 * f9;

        let f4 = table[table_index * 2 + 3];
        let f5 = table[table_index * 2 + 2];
        let f10 = f5 - f4 * (f3 + f3);
        let f9 = (f3 + f3) * f5 - f4;

        let x8 = data[base + 8];
        let x9 = data[base + 9];
        let x10 = data[base + 10];
        let x11 = data[base + 11];
        let x12 = data[base + 12];
        let x13 = data[base + 13];
        let x14 = data[base + 14];
        let x15 = data[base + 15];
        let f11 = x9 + x11;
        let f12 = x9 - x11;
        let f13 = x13 + x15;
        let f14 = x13 - x15;
        let f15 = f11 - f13;
        data[base + 8] = x8 + x10 + x12 + x14;
        let f16 = f12 - (x12 - x14);
        data[base + 9] = f11 + f13;
        let f6 = (x8 + x10) - (x12 + x14);
        data[base + 12] = -f2 * f6 - f3 * f15;
        let f11 = (x8 - x10) - f14;
        data[base + 13] = -f2 * f15 + f3 * f6;
        let f12 = f12 + (x12 - x14);
        let f14 = (x8 - x10) + f14;
        data[base + 10] = f5 * f11 - f4 * f12;
        data[base + 11] = f4 * f11 + f5 * f12;
        data[base + 14] = f10 * f14 - f9 * f16;
        data[base + 15] = f16 * f10 + f9 * f14;

        if cursor >= count {
            break;
        }
    }
}

fn rdftv_staged_butterflies(data: &mut [f32], table: &[f32], count: usize) -> usize {
    let mut span = 8usize;
    if count <= 0x20 {
        return span;
    }

    loop {
        rdftv_stage_base_quadrants(data, span);
        rdftv_stage_rotated_quadrants(data, span, table[2]);
        rdftv_stage_outer_quadrants(data, table, count, span);

        let next_loop_limit = span << 4;
        span *= 4;
        if next_loop_limit >= count {
            break;
        }
    }

    span
}

fn rdftv_stage_base_quadrants(data: &mut [f32], span: usize) {
    let mut idx = 0usize;
    while idx < span {
        let p1 = span + idx;
        let p2 = span * 2 + idx;
        let p3 = span * 3 + idx;

        let f2 = data[idx] + data[p1];
        let f3 = data[idx] - data[p1];
        let f7 = data[idx + 1] + data[p1 + 1];
        let f6 = data[idx + 1] - data[p1 + 1];
        let f8 = data[p2] + data[p3];
        let f5 = data[p2 + 1] + data[p3 + 1];
        let f4 = data[p2 + 1] - data[p3 + 1];
        let f9 = data[p2] - data[p3];
        data[idx] = f2 + f8;
        data[idx + 1] = f7 + f5;
        data[p2] = f2 - f8;
        data[p2 + 1] = f7 - f5;
        data[p1] = f3 - f4;
        data[p1 + 1] = f6 + f9;
        data[p3] = f4 + f3;
        data[p3 + 1] = f6 - f9;

        idx += 2;
    }
}

fn rdftv_stage_rotated_quadrants(data: &mut [f32], span: usize, scale: f32) {
    let mut idx = span * 4;
    let end = span * 5;
    while idx < end {
        let p1 = span + idx;
        let p2 = span * 2 + idx;
        let p3 = span * 3 + idx;

        let f3 = data[idx] + data[p1];
        let f4 = data[idx] - data[p1];
        let f7 = data[idx + 1] + data[p1 + 1];
        let f8 = data[idx + 1] - data[p1 + 1];
        let f9 = data[p2] + data[p3];
        let f10 = data[p2 + 1] + data[p3 + 1];
        let f5 = data[p2 + 1] - data[p3 + 1];
        let f6 = data[p2] - data[p3];
        data[idx] = f3 + f9;
        data[idx + 1] = f7 + f10;
        data[p2] = f10 - f7;
        data[p2 + 1] = f3 - f9;
        let f3 = f4 - f5;
        let f7 = f8 + f6;
        let f5 = f5 + f4;
        data[p1] = (f3 - f7) * scale;
        let f6 = f6 - f8;
        data[p1 + 1] = scale * (f7 + f3);
        data[p3] = (f6 - f5) * scale;
        data[p3 + 1] = scale * (f5 + f6);

        idx += 2;
    }
}

fn rdftv_stage_outer_quadrants(data: &mut [f32], table: &[f32], count: usize, span: usize) {
    let block = span * 8;
    if block >= count {
        return;
    }

    let first_rotated_start = span * 4;
    let first_rotated_end = span * 5;
    let mut table_index = 0usize;
    let mut base = block;
    while base < count {
        table_index += 2;
        let f2 = table[table_index];
        let f3 = table[table_index + 1];
        let f4 = table[table_index * 2 + 1];
        let f5 = table[table_index * 2];
        let f7 = f5 - (f3 + f3) * f4;
        let f6 = (f3 + f3) * f5 - f4;

        let first_end = base + span;
        let mut idx = base;
        while idx < first_end {
            let p1 = span + idx;
            let p2 = span * 2 + idx;
            let p3 = span * 3 + idx;

            let f11 = data[idx] + data[p1];
            let f8 = data[p2];
            let f15 = data[idx + 1] + data[p1 + 1];
            let f12 = data[idx + 1] - data[p1 + 1];
            let f10 = data[p3] + f8;
            let f16 = data[idx] - data[p1];
            let f14 = data[p2 + 1] + data[p3 + 1];
            let f13 = data[p2 + 1] - data[p3 + 1];
            let f9 = data[p3];
            data[idx] = f11 + f10;
            let f11 = f11 - f10;
            data[idx + 1] = f15 + f14;
            let f15 = f15 - f14;
            let f8 = f8 - f9;
            data[p2] = f2 * f11 - f3 * f15;
            let f9 = f12 + f8;
            let f12 = f12 - f8;
            data[p2 + 1] = f2 * f15 + f3 * f11;
            let f8 = f16 - f13;
            let f13 = f13 + f16;
            data[p1] = f5 * f8 - f4 * f9;
            data[p1 + 1] = f5 * f9 + f4 * f8;
            data[p3] = f7 * f13 - f6 * f12;
            data[p3 + 1] = f6 * f13 + f7 * f12;

            idx += 2;
        }

        let f4 = table[table_index * 2 + 2];
        let f5 = table[table_index * 2 + 3];
        let f7 = f4 - (f2 + f2) * f5;
        let f6 = (f2 + f2) * f4 - f5;
        let mut idx = base + first_rotated_start;
        let second_end = base + first_rotated_end;
        while idx < second_end {
            let p1 = span + idx;
            let p2 = span * 2 + idx;
            let p3 = span * 3 + idx;

            let f8 = data[p1] + data[idx];
            let f9 = data[idx] - data[p1];
            let f10 = data[idx + 1] + data[p1 + 1];
            let f11 = data[idx + 1] - data[p1 + 1];
            let f12 = data[p3] + data[p2];
            let f15 = data[p2 + 1] + data[p3 + 1];
            let f16 = data[p2 + 1] - data[p3 + 1];
            let f13 = data[p2] - data[p3];
            let f14 = f8 - f12;
            data[idx] = f8 + f12;
            data[idx + 1] = f10 + f15;
            let f10 = f10 - f15;
            data[p2] = -f3 * f14 - f2 * f10;
            let f8 = f11 + f13;
            data[p2 + 1] = -f3 * f10 + f2 * f14;
            let f10 = f9 - f16;
            let f16 = f16 + f9;
            data[p1] = f4 * f10 - f5 * f8;
            let f11 = f11 - f13;
            data[p1 + 1] = f4 * f8 + f5 * f10;
            data[p3] = f7 * f16 - f6 * f11;
            data[p3 + 1] = f7 * f11 + f6 * f16;

            idx += 2;
        }

        base += block;
    }
}

fn rdftv_final_butterfly(data: &mut [f32], count: usize, span: usize) {
    if span * 4 == count {
        let mut idx = 0usize;
        while idx < span {
            let p1 = span + idx;
            let p2 = span * 2 + idx;
            let p3 = span * 3 + idx;

            let f2 = data[p2];
            let f3 = data[p3];
            let f6 = data[idx] + data[p1];
            let f11 = data[idx + 1] + data[p1 + 1];
            let f10 = data[idx + 1] - data[p1 + 1];
            let f4 = data[p2];
            let f9 = data[p2 + 1] + data[p3 + 1];
            let f8 = data[p2 + 1] - data[p3 + 1];
            let f5 = data[p3];
            let f7 = data[idx] - data[p1];
            data[idx] = f6 + f2 + f3;
            data[idx + 1] = f11 + f9;
            data[p2] = f6 - (f2 + f3);
            data[p2 + 1] = f11 - f9;
            data[p1] = f7 - f8;
            data[p1 + 1] = f10 + (f4 - f5);
            data[p3] = f7 + f8;
            data[p3 + 1] = f10 - (f4 - f5);

            idx += 2;
        }
    } else {
        let mut idx = 0usize;
        while idx < span {
            let p1 = span + idx;
            let f2 = data[p1];
            let f3 = data[idx];
            let f4 = data[p1 + 1];
            let f5 = data[idx + 1];
            data[idx] = f3 + f2;
            data[idx + 1] = f4 + f5;
            data[p1] = f3 - f2;
            data[p1 + 1] = f5 - f4;

            idx += 2;
        }
    }
}

fn rdftv_real_postprocess(data: &mut [f32], table: &[f32], count: usize) {
    let half = count >> 1;
    let mut cursor = 2usize;
    let mut quarter = count >> 2;
    if half > 2 {
        let mut high = count - 2;
        let mut low = 3usize;
        let mut table_tail = half;
        loop {
            table_tail -= 1;
            cursor += 2;
            let f3 = 0.5 - table[table_tail];
            quarter += 1;
            let f2 = table[quarter];
            let f4 = data[low - 1] - data[high];
            let f6 = data[low] + data[high + 1];
            let f5 = f3 * f4 - f2 * f6;
            let f2 = f3 * f6 + f2 * f4;
            data[low - 1] -= f5;
            data[low] -= f2;
            data[high] += f5;
            data[high + 1] -= f2;

            if cursor >= half {
                break;
            }
            low += 2;
            high -= 2;
        }
    }

    let f2 = data[1];
    data[1] = data[0] - f2;
    data[0] += f2;
}
