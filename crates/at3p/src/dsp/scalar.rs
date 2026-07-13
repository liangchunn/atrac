#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarError {
    SampleCountNotMultipleOfFour { sample_count: usize },
    InputTooShort { needed: usize, actual: usize },
    OutputTooShort { needed: usize, actual: usize },
}

pub fn cp_short_to_scalar_at5(
    dst: &mut [f32],
    src: &[i16],
    sample_count: usize,
) -> Result<(), ScalarError> {
    if sample_count % 4 != 0 {
        return Err(ScalarError::SampleCountNotMultipleOfFour { sample_count });
    }
    if src.len() < sample_count {
        return Err(ScalarError::InputTooShort {
            needed: sample_count,
            actual: src.len(),
        });
    }
    if dst.len() < sample_count {
        return Err(ScalarError::OutputTooShort {
            needed: sample_count,
            actual: dst.len(),
        });
    }

    for (dst_chunk, src_chunk) in dst[..sample_count]
        .chunks_exact_mut(4)
        .zip(src[..sample_count].chunks_exact(4))
    {
        dst_chunk[0] = f32::from(src_chunk[0]);
        dst_chunk[1] = f32::from(src_chunk[1]);
        dst_chunk[2] = f32::from(src_chunk[2]);
        dst_chunk[3] = f32::from(src_chunk[3]);
    }

    Ok(())
}

pub fn add_seq_at5(a: &[f32], b: &[f32], dst: &mut [f32], count: usize) -> Result<(), ScalarError> {
    check_sequence_inputs(a.len(), b.len(), dst.len(), count)?;

    for ((a_chunk, b_chunk), dst_chunk) in a[..count]
        .chunks_exact(4)
        .zip(b[..count].chunks_exact(4))
        .zip(dst[..count].chunks_exact_mut(4))
    {
        dst_chunk[0] = a_chunk[0] + b_chunk[0];
        dst_chunk[1] = a_chunk[1] + b_chunk[1];
        dst_chunk[2] = a_chunk[2] + b_chunk[2];
        dst_chunk[3] = a_chunk[3] + b_chunk[3];
    }

    Ok(())
}

pub fn add_seq_at5_in_place_a(a: &mut [f32], b: &[f32], count: usize) -> Result<(), ScalarError> {
    check_sequence_inputs(a.len(), b.len(), a.len(), count)?;

    for (a_chunk, b_chunk) in a[..count]
        .chunks_exact_mut(4)
        .zip(b[..count].chunks_exact(4))
    {
        a_chunk[0] += b_chunk[0];
        a_chunk[1] += b_chunk[1];
        a_chunk[2] += b_chunk[2];
        a_chunk[3] += b_chunk[3];
    }

    Ok(())
}

pub fn sub_seq_at5(a: &[f32], b: &[f32], dst: &mut [f32], count: usize) -> Result<(), ScalarError> {
    check_sequence_inputs(a.len(), b.len(), dst.len(), count)?;

    for ((a_chunk, b_chunk), dst_chunk) in a[..count]
        .chunks_exact(4)
        .zip(b[..count].chunks_exact(4))
        .zip(dst[..count].chunks_exact_mut(4))
    {
        dst_chunk[0] = a_chunk[0] - b_chunk[0];
        dst_chunk[1] = a_chunk[1] - b_chunk[1];
        dst_chunk[2] = a_chunk[2] - b_chunk[2];
        dst_chunk[3] = a_chunk[3] - b_chunk[3];
    }

    Ok(())
}

pub fn sub_seq_at5_in_place_a(a: &mut [f32], b: &[f32], count: usize) -> Result<(), ScalarError> {
    check_sequence_inputs(a.len(), b.len(), a.len(), count)?;

    for (a_chunk, b_chunk) in a[..count]
        .chunks_exact_mut(4)
        .zip(b[..count].chunks_exact(4))
    {
        a_chunk[0] -= b_chunk[0];
        a_chunk[1] -= b_chunk[1];
        a_chunk[2] -= b_chunk[2];
        a_chunk[3] -= b_chunk[3];
    }

    Ok(())
}

pub fn mix_seq_at5(a: &[f32], b: &[f32], dst: &mut [f32], count: usize) -> Result<(), ScalarError> {
    check_sequence_inputs(a.len(), b.len(), dst.len(), count)?;

    for ((a_chunk, b_chunk), dst_chunk) in a[..count]
        .chunks_exact(4)
        .zip(b[..count].chunks_exact(4))
        .zip(dst[..count].chunks_exact_mut(4))
    {
        dst_chunk[0] = (a_chunk[0] + b_chunk[0]) * 0.5;
        dst_chunk[1] = (a_chunk[1] + b_chunk[1]) * 0.5;
        dst_chunk[2] = (a_chunk[2] + b_chunk[2]) * 0.5;
        dst_chunk[3] = (a_chunk[3] + b_chunk[3]) * 0.5;
    }

    Ok(())
}

pub fn invmix_seq_at5(
    a: &[f32],
    b: &[f32],
    dst: &mut [f32],
    count: usize,
) -> Result<(), ScalarError> {
    check_sequence_inputs(a.len(), b.len(), dst.len(), count)?;

    for ((a_chunk, b_chunk), dst_chunk) in a[..count]
        .chunks_exact(4)
        .zip(b[..count].chunks_exact(4))
        .zip(dst[..count].chunks_exact_mut(4))
    {
        dst_chunk[0] = (a_chunk[0] - b_chunk[0]) * 0.5;
        dst_chunk[1] = (a_chunk[1] - b_chunk[1]) * 0.5;
        dst_chunk[2] = (a_chunk[2] - b_chunk[2]) * 0.5;
        dst_chunk[3] = (a_chunk[3] - b_chunk[3]) * 0.5;
    }

    Ok(())
}

fn check_sequence_inputs(
    a_len: usize,
    b_len: usize,
    dst_len: usize,
    count: usize,
) -> Result<(), ScalarError> {
    if count % 4 != 0 {
        return Err(ScalarError::SampleCountNotMultipleOfFour {
            sample_count: count,
        });
    }
    if a_len < count {
        return Err(ScalarError::InputTooShort {
            needed: count,
            actual: a_len,
        });
    }
    if b_len < count {
        return Err(ScalarError::InputTooShort {
            needed: count,
            actual: b_len,
        });
    }
    if dst_len < count {
        return Err(ScalarError::OutputTooShort {
            needed: count,
            actual: dst_len,
        });
    }

    Ok(())
}
