use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatTraceClass {
    Intermediate(FloatTolerance),
    DecisionBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatTolerance {
    pub max_abs: f64,
    pub max_rel: f64,
}

impl FloatTolerance {
    pub const fn new(max_abs: f64, max_rel: f64) -> Self {
        Self { max_abs, max_rel }
    }

    fn is_valid(self) -> bool {
        self.max_abs.is_finite()
            && self.max_rel.is_finite()
            && self.max_abs >= 0.0
            && self.max_rel >= 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatDifference {
    pub index: usize,
    pub native: f32,
    pub rust: f32,
    pub native_bits: u32,
    pub rust_bits: u32,
    pub abs_error: f64,
    pub rel_error: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloatComparisonReport {
    pub class: FloatTraceClass,
    pub len: usize,
    pub max_abs_error: f64,
    pub max_abs_error_index: Option<usize>,
    pub max_rel_error: f64,
    pub max_rel_error_index: Option<usize>,
    pub first_difference: Option<FloatDifference>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FloatTraceError {
    LengthMismatch { native_len: usize, rust_len: usize },
    InvalidTolerance(FloatTolerance),
    DecisionBoundaryMismatch(FloatComparisonReport),
    IntermediateToleranceExceeded(FloatComparisonReport),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceElement {
    F32Le,
    I16Le,
    U16Le,
    I32Le,
    U32Le,
}

impl TraceElement {
    pub const fn byte_len(self) -> usize {
        match self {
            Self::F32Le | Self::I32Le | Self::U32Le => 4,
            Self::I16Le | Self::U16Le => 2,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::F32Le => "f32_le",
            Self::I16Le => "i16_le",
            Self::U16Le => "u16_le",
            Self::I32Le => "i32_le",
            Self::U32Le => "u32_le",
        }
    }
}

#[derive(Debug)]
pub enum TraceArrayError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    UnalignedByteLength {
        element: TraceElement,
        byte_len: usize,
        element_size: usize,
    },
    UnexpectedElementCount {
        element: TraceElement,
        expected: usize,
        actual: usize,
        byte_len: usize,
    },
}

pub fn compare_f32_trace(
    native: &[f32],
    rust: &[f32],
    class: FloatTraceClass,
) -> Result<FloatComparisonReport, FloatTraceError> {
    if native.len() != rust.len() {
        return Err(FloatTraceError::LengthMismatch {
            native_len: native.len(),
            rust_len: rust.len(),
        });
    }

    if let FloatTraceClass::Intermediate(tolerance) = class {
        if !tolerance.is_valid() {
            return Err(FloatTraceError::InvalidTolerance(tolerance));
        }
    }

    let report = build_report(native, rust, class);

    match class {
        FloatTraceClass::DecisionBoundary => {
            if report.first_difference.is_none() {
                Ok(report)
            } else {
                Err(FloatTraceError::DecisionBoundaryMismatch(report))
            }
        }
        FloatTraceClass::Intermediate(_) => {
            if report.first_difference.is_none() {
                Ok(report)
            } else {
                Err(FloatTraceError::IntermediateToleranceExceeded(report))
            }
        }
    }
}

pub fn read_f32_le_trace_array(
    path: impl AsRef<Path>,
    expected_len: usize,
) -> Result<Vec<f32>, TraceArrayError> {
    let bytes = read_trace_array_bytes(path, expected_len, TraceElement::F32Le)?;
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}

pub fn read_i16_le_trace_array(
    path: impl AsRef<Path>,
    expected_len: usize,
) -> Result<Vec<i16>, TraceArrayError> {
    let bytes = read_trace_array_bytes(path, expected_len, TraceElement::I16Le)?;
    Ok(bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}

pub fn read_u16_le_trace_array(
    path: impl AsRef<Path>,
    expected_len: usize,
) -> Result<Vec<u16>, TraceArrayError> {
    let bytes = read_trace_array_bytes(path, expected_len, TraceElement::U16Le)?;
    Ok(bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}

pub fn read_i32_le_trace_array(
    path: impl AsRef<Path>,
    expected_len: usize,
) -> Result<Vec<i32>, TraceArrayError> {
    let bytes = read_trace_array_bytes(path, expected_len, TraceElement::I32Le)?;
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}

pub fn read_u32_le_trace_array(
    path: impl AsRef<Path>,
    expected_len: usize,
) -> Result<Vec<u32>, TraceArrayError> {
    let bytes = read_trace_array_bytes(path, expected_len, TraceElement::U32Le)?;
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}

fn build_report(native: &[f32], rust: &[f32], class: FloatTraceClass) -> FloatComparisonReport {
    let mut first_difference = None;
    let mut max_abs_error = 0.0_f64;
    let mut max_abs_error_index = None;
    let mut max_rel_error = 0.0_f64;
    let mut max_rel_error_index = None;

    for (index, (&native_value, &rust_value)) in native.iter().zip(rust).enumerate() {
        let difference = difference(index, native_value, rust_value);

        if difference.abs_error > max_abs_error {
            max_abs_error = difference.abs_error;
            max_abs_error_index = Some(index);
        }
        if difference.rel_error > max_rel_error {
            max_rel_error = difference.rel_error;
            max_rel_error_index = Some(index);
        }

        if first_difference.is_none() && values_differ(native_value, rust_value, class) {
            first_difference = Some(difference);
        }
    }

    FloatComparisonReport {
        class,
        len: native.len(),
        max_abs_error,
        max_abs_error_index,
        max_rel_error,
        max_rel_error_index,
        first_difference,
    }
}

fn difference(index: usize, native: f32, rust: f32) -> FloatDifference {
    let abs_error = if native.to_bits() == rust.to_bits() {
        0.0
    } else if native.is_finite() && rust.is_finite() {
        (f64::from(native) - f64::from(rust)).abs()
    } else {
        f64::INFINITY
    };
    let rel_error = if abs_error == 0.0 {
        0.0
    } else {
        let scale = f64::from(native).abs().max(f64::MIN_POSITIVE);
        abs_error / scale
    };

    FloatDifference {
        index,
        native,
        rust,
        native_bits: native.to_bits(),
        rust_bits: rust.to_bits(),
        abs_error,
        rel_error,
    }
}

fn values_differ(native: f32, rust: f32, class: FloatTraceClass) -> bool {
    match class {
        FloatTraceClass::DecisionBoundary => native.to_bits() != rust.to_bits(),
        FloatTraceClass::Intermediate(tolerance) => {
            let difference = difference(0, native, rust);
            difference.abs_error > tolerance.max_abs && difference.rel_error > tolerance.max_rel
        }
    }
}

fn read_trace_array_bytes(
    path: impl AsRef<Path>,
    expected_len: usize,
    element: TraceElement,
) -> Result<Vec<u8>, TraceArrayError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| TraceArrayError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let element_size = element.byte_len();

    if bytes.len() % element_size != 0 {
        return Err(TraceArrayError::UnalignedByteLength {
            element,
            byte_len: bytes.len(),
            element_size,
        });
    }

    let actual = bytes.len() / element_size;
    if actual != expected_len {
        return Err(TraceArrayError::UnexpectedElementCount {
            element,
            expected: expected_len,
            actual,
            byte_len: bytes.len(),
        });
    }

    Ok(bytes)
}
