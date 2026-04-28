//! aarch64 NEON implementations. Gated by `cfg(target_arch = "aarch64")`;
//! the dispatcher only calls these after
//! `is_aarch64_feature_detected!("neon")` returns true (which is
//! basically always on Apple Silicon and modern ARM cores).

#[target_feature(enable = "neon")]
pub(super) unsafe fn dot_f32_neon(q: &[f32], row_bytes: &[u8]) -> f32 {
    unsafe {
        use std::arch::aarch64::*;
        let dim = q.len();
        let row_ptr = row_bytes.as_ptr() as *const f32;
        let mut acc = vdupq_n_f32(0.0);
        let chunks = dim / 4;
        for i in 0..chunks {
            let qv = vld1q_f32(q.as_ptr().add(i * 4));
            let rv = vld1q_f32(row_ptr.add(i * 4));
            acc = vfmaq_f32(acc, qv, rv);
        }
        let mut tail = 0.0f32;
        for (i, &qv) in q.iter().enumerate().skip(chunks * 4) {
            let off = i * 4;
            let v = f32::from_le_bytes([
                row_bytes[off],
                row_bytes[off + 1],
                row_bytes[off + 2],
                row_bytes[off + 3],
            ]);
            tail += qv * v;
        }
        let lane_sum = vaddvq_f32(acc);
        lane_sum + tail
    }
}

// `float16x4_t` and `vcvt_f32_f16` are stable since rustc 1.94. Workspace
// MSRV is 1.85, but only this aarch64-only function uses them. Suppress
// the lint here rather than bumping the whole workspace's MSRV.
#[allow(clippy::incompatible_msrv)]
#[target_feature(enable = "neon")]
pub(super) unsafe fn dot_f32_f16_neon(q: &[f32], row_bytes: &[u8]) -> f32 {
    unsafe {
        // NEON has fcvtl to widen f16 -> f32 in groups of 4. Pack 4 lanes per
        // step. half::f16 layout matches IEEE binary16, same as ARM f16.
        use std::arch::aarch64::*;
        let dim = q.len();
        let row_ptr = row_bytes.as_ptr() as *const u16;
        let mut acc = vdupq_n_f32(0.0);
        let chunks = dim / 4;
        for i in 0..chunks {
            let halfs = vld1_u16(row_ptr.add(i * 4));
            // Reinterpret as float16x4_t and widen.
            let f16x4: float16x4_t = std::mem::transmute(halfs);
            let widened: float32x4_t = vcvt_f32_f16(f16x4);
            let qv = vld1q_f32(q.as_ptr().add(i * 4));
            acc = vfmaq_f32(acc, qv, widened);
        }
        let mut tail = 0.0f32;
        for (i, &qv) in q.iter().enumerate().skip(chunks * 4) {
            let off = i * 2;
            let h = half::f16::from_le_bytes([row_bytes[off], row_bytes[off + 1]]);
            tail += qv * h.to_f32();
        }
        let lane_sum = vaddvq_f32(acc);
        lane_sum + tail
    }
}

#[target_feature(enable = "neon")]
pub(super) unsafe fn dot_f32_i8_neon(q: &[f32], row: &[i8]) -> f32 {
    unsafe {
        use std::arch::aarch64::*;
        let dim = q.len();
        let mut acc = vdupq_n_f32(0.0);
        // Process 8 lanes per step (NEON's i8x8 widens cleanly to i16x8 then
        // i32x4 + i32x4, then to f32x4 + f32x4).
        let chunks = dim / 8;
        for i in 0..chunks {
            let i8x8 = vld1_s8(row.as_ptr().add(i * 8));
            let i16x8 = vmovl_s8(i8x8);
            let i32_lo = vmovl_s16(vget_low_s16(i16x8));
            let i32_hi = vmovl_s16(vget_high_s16(i16x8));
            let f_lo = vcvtq_f32_s32(i32_lo);
            let f_hi = vcvtq_f32_s32(i32_hi);
            let q_lo = vld1q_f32(q.as_ptr().add(i * 8));
            let q_hi = vld1q_f32(q.as_ptr().add(i * 8 + 4));
            acc = vfmaq_f32(acc, q_lo, f_lo);
            acc = vfmaq_f32(acc, q_hi, f_hi);
        }
        let mut tail = 0.0f32;
        for i in (chunks * 8)..dim {
            tail += q[i] * (row[i] as f32);
        }
        vaddvq_f32(acc) + tail
    }
}
