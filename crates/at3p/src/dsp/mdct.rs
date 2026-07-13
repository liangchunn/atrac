use crate::tables::at5::{coef_c_at5, coef_s_at5, matrix_at5};

pub const MDCT_128_INPUT_COUNT: usize = 256;
pub const MDCT_128_OUTPUT_COUNT: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdctError {
    InputTooShort { needed: usize, actual: usize },
    OutputTooShort { needed: usize, actual: usize },
    WindowTooShort { needed: usize, actual: usize },
    UnsupportedOutputOrder { output_order: usize },
}

pub fn winormal_mdct_128_ex_at5(
    input: &[f32],
    output: &mut [f32],
    window: &[f32],
    output_order: usize,
) -> Result<(), MdctError> {
    if input.len() < MDCT_128_INPUT_COUNT {
        return Err(MdctError::InputTooShort {
            needed: MDCT_128_INPUT_COUNT,
            actual: input.len(),
        });
    }
    if output.len() < MDCT_128_OUTPUT_COUNT {
        return Err(MdctError::OutputTooShort {
            needed: MDCT_128_OUTPUT_COUNT,
            actual: output.len(),
        });
    }
    if window.len() < MDCT_128_INPUT_COUNT {
        return Err(MdctError::WindowTooShort {
            needed: MDCT_128_INPUT_COUNT,
            actual: window.len(),
        });
    }
    if output_order > 1 {
        return Err(MdctError::UnsupportedOutputOrder { output_order });
    }

    let coef_c = coef_c_at5();
    let coef_s = coef_s_at5();
    let matrix = matrix_at5();
    let mut scratch = [0.0f32; MDCT_128_OUTPUT_COUNT];

    mdct_window_permute(
        &input[..MDCT_128_INPUT_COUNT],
        &window[..MDCT_128_INPUT_COUNT],
        &matrix,
        &mut scratch,
    );
    let coef_index = mdct_butterflies(&mut scratch, &coef_c, &coef_s);
    mdct_store_output(
        &scratch,
        &coef_c,
        &coef_s,
        &mut output[..MDCT_128_OUTPUT_COUNT],
        coef_index,
        output_order,
    );

    Ok(())
}

fn mdct_window_permute(input: &[f32], window: &[f32], matrix: &[u16], scratch: &mut [f32; 128]) {
    let mut front = 0usize;
    let mut tail = 0xfcusize;
    while front < 0x40 {
        scratch[usize::from(matrix[64 + front])] =
            window[front] * input[front] - window[tail - 0x7d] * input[tail - 0x7d];
        scratch[usize::from(matrix[65 + front])] =
            window[front + 1] * input[front + 1] - window[tail - 0x7e] * input[tail - 0x7e];
        scratch[usize::from(matrix[66 + front])] =
            window[front + 2] * input[front + 2] - window[tail - 0x7f] * input[tail - 0x7f];
        scratch[usize::from(matrix[67 + front])] =
            window[front + 3] * input[front + 3] - window[tail - 0x80] * input[tail - 0x80];

        scratch[usize::from(matrix[tail - 0xbd])] =
            -input[front + 0x80] * window[front + 0x80] - window[tail + 3] * input[tail + 3];
        scratch[usize::from(matrix[tail - 0xbe])] =
            -input[front + 0x81] * window[front + 0x81] - window[tail + 2] * input[tail + 2];
        scratch[usize::from(matrix[tail - 0xbf])] =
            -input[front + 0x82] * window[front + 0x82] - window[tail + 1] * input[tail + 1];
        scratch[usize::from(matrix[tail - 0xc0])] =
            -input[front + 0x83] * window[front + 0x83] - window[tail] * input[tail];

        front += 4;
        tail -= 4;
    }
}

fn mdct_butterflies(scratch: &mut [f32; 128], coef_c: &[f32; 128], coef_s: &[f32; 128]) -> usize {
    let mut coef_index = 0usize;
    let mut stage = 0usize;
    while stage < 6 {
        let half_span = 1usize << stage;
        let groups = 0x80 / (half_span << 2);
        let span = 1usize << (stage + 1);
        let mut left = 0usize;
        let mut right = span;

        for _ in 0..groups {
            for _ in 0..half_span {
                let left_real = scratch[left];
                let left_imag = scratch[left + 1];
                let coef_c_value = coef_c[coef_index];
                let coef_s_value = coef_s[coef_index];
                let right_real = scratch[right];
                let right_imag = scratch[right + 1];
                coef_index += 1;

                let twiddled_real = coef_c_value * right_real + coef_s_value * right_imag;
                let twiddled_imag = coef_s_value * right_real - coef_c_value * right_imag;
                scratch[left] = left_real + twiddled_real;
                scratch[left + 1] = left_imag + twiddled_imag;
                left += 2;
                scratch[right] = left_real - twiddled_real;
                scratch[right + 1] = left_imag - twiddled_imag;
                right += 2;
            }
            coef_index -= half_span;
            left += span;
            right += span;
        }
        coef_index += half_span;
        stage += 1;
    }

    coef_index
}

fn mdct_store_output(
    scratch: &[f32; 128],
    coef_c: &[f32; 128],
    coef_s: &[f32; 128],
    output: &mut [f32],
    mut coef_index: usize,
    output_order: usize,
) {
    const SCALE: f32 = 0.015625;

    let mut group = 0usize;
    while group < 0x40 {
        let base = group * 2;
        for lane in 0..4 {
            let index = base + lane * 2;
            let real = scratch[index];
            let imag = scratch[index + 1];
            let sum = (coef_c[coef_index] * real + coef_s[coef_index] * imag) * SCALE;
            let diff = (real * coef_s[coef_index] - imag * coef_c[coef_index]) * SCALE;
            let mirror = 0x7f - index;

            if output_order == 0 {
                output[index] = sum;
                output[mirror] = diff;
            } else {
                output[mirror] = sum;
                output[index] = diff;
            }

            coef_index += 1;
        }
        group += 4;
    }
}
