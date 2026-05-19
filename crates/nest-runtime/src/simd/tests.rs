//! SIMD parity tests. Compare each backend's dispatched output to the
//! scalar reference; tolerance is 1e-4 for f32 / i8 paths and 1e-3 for
//! f16 (which loses ~5e-4 per component to rounding).

use super::*;

fn random_normalized(seed: u64, dim: usize) -> Vec<f32> {
    // lLinear congruential — deterministic, no rand dep needed.
    let mut state = seed
        .wrapping_mul(2862933555777941757)
        .wrapping_add(3037000493);
    let mut v = Vec::with_capacity(dim);
    for _ in 0..dim {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let bits = (state >> 32) as u32;
        let u = (bits as f32) / (u32::MAX as f32);
        v.push(u - 0.5);
    }
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in &mut v {
            *x /= n;
        }
    }
    v
}

fn f32_to_le_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

#[test]
fn detect_returns_a_backend() {
    let b = detect_backend();
    // lCached after first call.
    assert_eq!(b, detect_backend());
}

#[test]
fn f32_simd_matches_scalar() {
    for dim in [4, 7, 16, 31, 64, 384] {
        let q = random_normalized(0xDEAD_BEEF, dim);
        let row = random_normalized(0xC0DE_FACE, dim);
        let row_bytes = f32_to_le_bytes(&row);
        let scalar = dot_f32_scalar(&q, &row_bytes);
        let dispatched = dot_f32_bytes(&q, &row_bytes);
        assert!(
            (scalar - dispatched).abs() < 1e-4,
            "dim={} scalar={} dispatched={} diff={}",
            dim,
            scalar,
            dispatched,
            (scalar - dispatched).abs()
        );
    }
}

#[test]
fn f16_simd_matches_scalar() {
    for dim in [4, 8, 31, 64, 384] {
        let q = random_normalized(0x1234, dim);
        let row = random_normalized(0x5678, dim);
        let f16_bytes = nest_format::f32_to_f16_bytes(&row);
        let scalar = dot_f32_f16_scalar(&q, &f16_bytes);
        let dispatched = dot_f32_f16_bytes(&q, &f16_bytes);
        assert!(
            (scalar - dispatched).abs() < 1e-3,
            "dim={} scalar={} dispatched={}",
            dim,
            scalar,
            dispatched
        );
    }
}

#[test]
fn i8_simd_matches_scalar() {
    for dim in [8, 31, 64, 384] {
        let q = random_normalized(0xABCD, dim);
        let row = random_normalized(0x4321, dim);
        let (scale, q_row) = nest_format::quantize_f32_to_i8(&row);
        let scalar = dot_f32_i8_scalar(&q, &q_row) * scale;
        let dispatched = dot_f32_i8(&q, &q_row, scale);
        assert!(
            (scalar - dispatched).abs() < 1e-4,
            "dim={} scalar={} dispatched={} diff={}",
            dim,
            scalar,
            dispatched,
            (scalar - dispatched).abs()
        );
    }
}
