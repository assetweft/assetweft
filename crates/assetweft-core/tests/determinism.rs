use assetweft_core::assurance::{
    DeterminismError::{DegenerateQuaternion, NonFinite, Overflow},
    QUANTIZATION_SCALE_1E6, canonical_f32, canonical_f64, canonical_matrix_4x4,
    canonical_quaternion, canonical_semantic_bytes, canonical_vec3, quantize_1e6,
    quantize_1e6_units, semantic_sha256_hex,
};

#[test]
fn signed_zero_has_one_encoding() {
    assert_eq!(canonical_f64(-0.0).unwrap().to_bits(), 0.0f64.to_bits());
    assert_eq!(canonical_f32(-0.0).unwrap().to_bits(), 0.0f32.to_bits());
    assert_eq!(quantize_1e6(-0.0).unwrap().to_bits(), 0.0f64.to_bits());
}

#[test]
fn rejects_non_finite_inputs_and_scaled_overflow() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(canonical_f64(bad), Err(NonFinite));
        assert_eq!(quantize_1e6_units(bad), Err(NonFinite));
        assert_eq!(canonical_quaternion([0.0, 0.0, 0.0, bad]), Err(NonFinite));
        assert_eq!(semantic_sha256_hex(&[bad]), Err(NonFinite));
    }
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(canonical_f32(bad), Err(NonFinite));
    }
    let limit = i64::MAX as f64 / QUANTIZATION_SCALE_1E6;
    assert_eq!(quantize_1e6_units(-limit), Ok(i64::MIN));
    for overflow in [limit, -2.0 * limit, f64::MAX] {
        assert_eq!(quantize_1e6_units(overflow), Err(Overflow));
    }
}

#[test]
fn rounds_ties_even_and_preserves_entry_order() {
    assert_eq!(quantize_1e6_units(0.5e-6), Ok(0));
    assert_eq!(quantize_1e6_units(1.5e-6), Ok(2));
    assert_eq!(quantize_1e6_units(2.5e-6), Ok(2));
    assert_eq!(quantize_1e6(-0.5e-6).unwrap().to_bits(), 0.0f64.to_bits());
    assert_eq!(
        canonical_vec3([1.5e-6, -1.5e-6, 0.5e-6]),
        Ok([2e-6, -2e-6, 0.0])
    );
    let mut matrix = [-0.0; 16];
    matrix[1] = -1.5e-6;
    matrix[15] = 1.5e-6;
    let mut expected = [0.0f64; 16];
    expected[1] = -2e-6;
    expected[15] = 2e-6;
    assert_eq!(
        canonical_matrix_4x4(matrix).unwrap().map(f64::to_bits),
        expected.map(f64::to_bits)
    );
    matrix[15] = f64::MAX;
    assert_eq!(canonical_matrix_4x4(matrix), Err(Overflow));
}

#[test]
fn quaternion_sign_is_canonical_even_after_w_rounds_to_zero() {
    for (q, expected) in [
        ([-2.0, 2.0, -2.0, -2.0], [0.5, -0.5, 0.5, 0.5]),
        ([1.0, 0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
        ([0.0, 1.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0]),
        ([0.0, 0.0, -1.0, 1e-7], [0.0, 0.0, 1.0, 0.0]),
    ] {
        let canonical = canonical_quaternion(q).unwrap();
        let opposite = canonical_quaternion(q.map(|value| -value)).unwrap();
        assert_eq!(canonical.map(f64::to_bits), expected.map(f64::to_bits));
        assert_eq!(canonical.map(f64::to_bits), opposite.map(f64::to_bits));
        assert_eq!(
            semantic_sha256_hex(&canonical).unwrap(),
            semantic_sha256_hex(&opposite).unwrap()
        );
    }
    for q in [[0.0; 4], [1e-12, 0.0, 0.0, 0.0], [f64::MAX, 0.0, 0.0, 0.0]] {
        assert_eq!(canonical_quaternion(q), Err(DegenerateQuaternion));
    }
}

#[test]
fn semantic_digest_pins_tag_byte_order_and_quantization() {
    let values = [1e-6, -1.5e-6, -0.0];
    let bytes = [1i64, -2, 0].map(i64::to_le_bytes).concat();
    assert_eq!(canonical_semantic_bytes(&values).unwrap(), bytes);
    let digest = semantic_sha256_hex(&values).unwrap();
    assert_eq!(
        digest,
        "6a6f72311004eda1523edfe60bb136bfb38587c1e08501e184b10741caebd930"
    );
    assert_eq!(
        semantic_sha256_hex(&[-0.0]).unwrap(),
        semantic_sha256_hex(&[0.0]).unwrap()
    );
    assert_ne!(digest, semantic_sha256_hex(&[1e-6, -1e-6, 0.0]).unwrap());
    assert_ne!(digest, semantic_sha256_hex(&[-1.5e-6, 1e-6, 0.0]).unwrap());
}
