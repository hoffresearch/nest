//! x86_64 AVX2+FMA implementations. Gated by `cfg(target_arch =
//! "x86_64")`; the dispatcher only calls these after
//! `is_x86_feature_detected!("avx2")` and `"fma"` both return true.

#[target_feature(enable = "avx2,fma")]
pub(super) unsafe fn dot_f32_avx2(q: &[f32], row_bytes: &[u8]) -> f32 {
    unsafe {
        use std::arch::x86_64::*;
        let dim = q.len();
        let mut acc = _mm256_setzero_ps();
        let row_ptr = row_bytes.as_ptr() as *const f32;
        let chunks = dim / 8;
        for i in 0..chunks {
            let qv = _mm256_loadu_ps(q.as_ptr().add(i * 8));
            let rv = _mm256_loadu_ps(row_ptr.add(i * 8));
            acc = _mm256_fmadd_ps(qv, rv, acc);
        }
        let mut tail = 0.0f32;
        for (i, &qv) in q.iter().enumerate().skip(chunks * 8) {
            let off = i * 4;
            let v = f32::from_le_bytes([
                row_bytes[off],
                row_bytes[off + 1],
                row_bytes[off + 2],
                row_bytes[off + 3],
            ]);
            tail += qv * v;
        }
        let mut buf = [0.0f32; 8];
        _mm256_storeu_ps(buf.as_mut_ptr(), acc);
        buf.iter().sum::<f32>() + tail
    }
}

#[target_feature(enable = "avx2,fma")]
pub(super) unsafe fn dot_f32_i8_avx2(q: &[f32], row: &[i8]) -> f32 {
    unsafe {
        use std::arch::x86_64::*;
        let dim = q.len();
        let mut acc = _mm256_setzero_ps();
        let chunks = dim / 8;
        for i in 0..chunks {
            let i8_ptr = row.as_ptr().add(i * 8) as *const i64;
            // lLoad 8 i8s (8 bytes) into the low half of an xmm.
            let raw = _mm_set1_epi64x(*i8_ptr);
            // lWiden i8 -> i32 (8 lanes).
            let widened = _mm256_cvtepi8_epi32(raw);
            let f = _mm256_cvtepi32_ps(widened);
            let qv = _mm256_loadu_ps(q.as_ptr().add(i * 8));
            acc = _mm256_fmadd_ps(qv, f, acc);
        }
        let mut tail = 0.0f32;
        for i in (chunks * 8)..dim {
            tail += q[i] * (row[i] as f32);
        }
        let mut buf = [0.0f32; 8];
        _mm256_storeu_ps(buf.as_mut_ptr(), acc);
        buf.iter().sum::<f32>() + tail
    }
}
