#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerCheckError {
    SampleCountNotMultipleOfFour { sample_count: usize },
    InputTooShort { needed: usize, actual: usize },
}

pub fn check_power_level_at5(a: &[f32], b: &[f32], count: usize) -> Result<f64, PowerCheckError> {
    check_inputs(&[a.len(), b.len()], count)?;

    let mut lane0 = 0.0_f64;
    let mut lane1 = 0.0_f64;
    let mut lane2 = 0.0_f64;
    let mut lane3 = 0.0_f64;

    for (a_chunk, b_chunk) in a[..count].chunks_exact(4).zip(b[..count].chunks_exact(4)) {
        lane0 += f64::from(b_chunk[0]) * f64::from(a_chunk[0]);
        lane2 += f64::from(b_chunk[2]) * f64::from(a_chunk[2]);
        lane1 += f64::from(b_chunk[1]) * f64::from(a_chunk[1]);
        lane3 += f64::from(b_chunk[3]) * f64::from(a_chunk[3]);
    }

    Ok(lane0 + lane1 + lane2 + lane3)
}

pub fn check_power_level_dual_at5(
    a: &[f32],
    b: &[f32],
    c: &[f32],
    d: &[f32],
    count: usize,
) -> Result<[f32; 2], PowerCheckError> {
    check_inputs(&[a.len(), b.len(), c.len(), d.len()], count)?;

    let first = lane_power_f32(a, b, count);
    let second = lane_power_f32(c, d, count);

    Ok([first, second])
}

pub fn check_power_level_tripl_at5(
    a: &[f32],
    b: &[f32],
    c: &[f32],
    d: &[f32],
    e: &[f32],
    f: &[f32],
    count: usize,
) -> Result<[f32; 3], PowerCheckError> {
    check_inputs(
        &[a.len(), b.len(), c.len(), d.len(), e.len(), f.len()],
        count,
    )?;

    Ok([
        lane_power_f32(a, b, count),
        lane_power_f32(c, d, count),
        lane_power_f32(e, f, count),
    ])
}

pub struct ChannelCorrelation {
    pub db: Vec<f32>,
    pub a_power: Vec<f32>,
    pub b_power: Vec<f32>,
}

pub fn check_channel_correlation_at5(
    a_bands: &[&[f32]],
    b_bands: &[&[f32]],
    sample_count: usize,
    band_count: usize,
) -> Result<ChannelCorrelation, PowerCheckError> {
    if a_bands.len() < band_count || b_bands.len() < band_count {
        return Err(PowerCheckError::InputTooShort {
            needed: band_count,
            actual: a_bands.len().min(b_bands.len()),
        });
    }

    let mut result = ChannelCorrelation {
        db: Vec::with_capacity(band_count),
        a_power: Vec::with_capacity(band_count),
        b_power: Vec::with_capacity(band_count),
    };
    for band in 0..band_count {
        let a = a_bands[band];
        let b = b_bands[band];
        check_inputs(&[a.len(), b.len()], sample_count)?;
        let mut difference = vec![0.0f32; sample_count];
        for (dst, (&a_sample, &b_sample)) in difference.iter_mut().zip(a.iter().zip(b.iter())) {
            *dst = a_sample - b_sample;
        }
        let [a_power, b_power, difference_power] =
            check_power_level_tripl_at5(a, a, b, b, &difference, &difference, sample_count)?;

        let ratio = if a_power == 0.0 && b_power == 0.0 {
            Some(0.001f32)
        } else if difference_power == 0.0 {
            Some(0.001f32)
        } else if a_power != 0.0 && b_power != 0.0 {
            Some(difference_power / a_power.max(b_power))
        } else {
            Some(1.0f32)
        };
        let db = match ratio {
            Some(ratio) if ratio > 0.0 => (f64::from(ratio).ln() as f32) * 8.685889f32,
            _ => -160.0f32,
        };

        result.db.push(if 60.0 < -db { 60.0 } else { -db });
        result.a_power.push(a_power);
        result.b_power.push(b_power);
    }

    Ok(result)
}

fn lane_power_f32(a: &[f32], b: &[f32], count: usize) -> f32 {
    let mut lane0 = 0.0_f32;
    let mut lane1 = 0.0_f32;
    let mut lane2 = 0.0_f32;
    let mut lane3 = 0.0_f32;

    for (a_chunk, b_chunk) in a[..count].chunks_exact(4).zip(b[..count].chunks_exact(4)) {
        lane0 += b_chunk[0] * a_chunk[0];
        lane2 += b_chunk[2] * a_chunk[2];
        lane1 += b_chunk[1] * a_chunk[1];
        lane3 += b_chunk[3] * a_chunk[3];
    }

    lane0 + lane1 + lane2 + lane3
}

fn check_inputs(lengths: &[usize], count: usize) -> Result<(), PowerCheckError> {
    if count % 4 != 0 {
        return Err(PowerCheckError::SampleCountNotMultipleOfFour {
            sample_count: count,
        });
    }

    for &actual in lengths {
        if actual < count {
            return Err(PowerCheckError::InputTooShort {
                needed: count,
                actual,
            });
        }
    }

    Ok(())
}
