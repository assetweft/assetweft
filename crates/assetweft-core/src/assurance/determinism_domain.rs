use sha2::{Digest, Sha256};
use std::{error::Error, fmt};

pub const QUANTIZATION_SCALE_1E6: f64 = 1_000_000.0;
const SEMANTIC_TAG: &[u8] = b"assetweft:semantic:1\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeterminismError {
    NonFinite,
    Overflow,
    DegenerateQuaternion,
}

impl fmt::Display for DeterminismError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NonFinite => "non-finite input",
            Self::Overflow => "quantized value is outside the i64 range",
            Self::DegenerateQuaternion => "quaternion has an invalid squared norm",
        })
    }
}

impl Error for DeterminismError {}

pub fn canonical_f64(value: f64) -> Result<f64, DeterminismError> {
    if !value.is_finite() {
        return Err(DeterminismError::NonFinite);
    }
    Ok(if value == 0.0 { 0.0 } else { value })
}

pub fn canonical_f32(value: f32) -> Result<f32, DeterminismError> {
    if !value.is_finite() {
        return Err(DeterminismError::NonFinite);
    }
    Ok(if value == 0.0 { 0.0 } else { value })
}

pub fn quantize_1e6_units(value: f64) -> Result<i64, DeterminismError> {
    let scaled = canonical_f64(value)? * QUANTIZATION_SCALE_1E6;
    if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled >= i64::MAX as f64 {
        return Err(DeterminismError::Overflow);
    }
    Ok(scaled.round_ties_even() as i64)
}

fn from_units(units: i64) -> f64 {
    units as f64 / QUANTIZATION_SCALE_1E6
}

pub fn quantize_1e6(value: f64) -> Result<f64, DeterminismError> {
    Ok(from_units(quantize_1e6_units(value)?))
}

pub fn canonical_vec3([x, y, z]: [f64; 3]) -> Result<[f64; 3], DeterminismError> {
    Ok([quantize_1e6(x)?, quantize_1e6(y)?, quantize_1e6(z)?])
}

pub fn canonical_matrix_4x4(mut matrix: [f64; 16]) -> Result<[f64; 16], DeterminismError> {
    for value in &mut matrix {
        *value = quantize_1e6(*value)?;
    }
    Ok(matrix)
}

/// Canonicalizes a rotation in glTF `[x, y, z, w]` order (#142).
pub fn canonical_quaternion(q: [f64; 4]) -> Result<[f64; 4], DeterminismError> {
    if q.iter().any(|value| !value.is_finite()) {
        return Err(DeterminismError::NonFinite);
    }
    let [x, y, z, w] = q;
    let norm_sq = x * x + y * y + z * z + w * w;
    if !norm_sq.is_finite() || norm_sq <= 1e-24 {
        return Err(DeterminismError::DegenerateQuaternion);
    }
    let inv = 1.0 / norm_sq.sqrt();
    let mut units = [0; 4];
    for (unit, value) in units.iter_mut().zip(q) {
        *unit = quantize_1e6_units(value * inv)?;
    }
    let [ix, iy, iz, iw] = units;
    if [iw, ix, iy, iz]
        .into_iter()
        .find(|&unit| unit != 0)
        .is_some_and(|unit| unit < 0)
    {
        units = units.map(|unit| -unit);
    }
    Ok(units.map(from_units))
}

pub fn canonical_semantic_bytes(values: &[f64]) -> Result<Vec<u8>, DeterminismError> {
    let mut bytes = Vec::new();
    for &value in values {
        bytes.extend_from_slice(&quantize_1e6_units(value)?.to_le_bytes());
    }
    Ok(bytes)
}

pub fn semantic_sha256_hex(values: &[f64]) -> Result<String, DeterminismError> {
    let mut sha = Sha256::new();
    sha.update(SEMANTIC_TAG);
    sha.update(canonical_semantic_bytes(values)?);
    Ok(sha
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
