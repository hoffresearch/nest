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

// lL`float16x4_t` and `vcvt_f32_f16` are stable since rustc 1.94. Workspace
// lLMSRV is 1.85, but only this aarch64-only function uses them. Suppress
// lLthe lint here rather than bumping the whole workspace's MSRV.
#[allow(clippy::incompatible_msrv)]
#[target_feature(enable = "neon")]
pub(super) unsafe fn dot_f32_f16_neon(q: &[f32], row_bytes: &[u8]) -> f32 {
    unsafe {
        // lNEON has fcvtl to widen f16 -> f32 in groups of 4. Pack 4 lanes per
        // step. half::f16 layout matches IEEE binary16, same as ARM f16.
        use std::arch::aarch64::*;
        let dim = q.len();
        let row_ptr = row_bytes.as_ptr() as *const u16;
        let mut acc = vdupq_n_f32(0.0);
        let chunks = dim / 4;
        for i in 0..chunks {
            let halfs = vld1_u16(row_ptr.add(i * 4));
            // lReinterpret as float16x4_t and widen.
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

/// lFused dequant + dot for int4 block-`block` codes. NEON unpacks the
/// nibbles 16-at-a-time (vqtbl-free: shift+mask+sign-extend on i8 lanes)
/// into an f32 scratch row, then runs the IDENTICAL per-group scalar
/// reduction `super::scalar::dot_f32_i4_blocked` over it. Decoding the
/// nibbles is the win; the reduction stays scalar so the result is
/// bit-for-bit equal to the scalar backend (float add is not associative,
/// so a lane-parallel reduction would diverge in the last ulp).
#[target_feature(enable = "neon")]
pub(super) unsafe fn dot_f32_i4_neon(
    q: &[f32],
    codes: &[u8],
    group_scales: &[f32],
    dim: usize,
    block: usize,
) -> f32 {
    unsafe {
        use std::arch::aarch64::*;
        // lUnpack `dim` nibbles into f32 lanes. Process 16 packed bytes (32
        // nibbles) per step. Low nibble of byte k -> lane 2k, high -> 2k+1.
        let mut scratch = vec![0.0f32; dim];
        let nbytes = dim / 2;
        let chunks = nbytes / 16;
        let lo_mask = vdupq_n_s8(0x0F);
        for c in 0..chunks {
            let packed = vld1q_u8(codes.as_ptr().add(c * 16));
            let packed_s = vreinterpretq_s8_u8(packed);
            // llow nibbles: mask then sign-extend by <<4 >>4 (arithmetic).
            let lo = vshrq_n_s8::<4>(vshlq_n_s8::<4>(vandq_s8(packed_s, lo_mask)));
            // lhigh nibbles: arithmetic shift right by 4 sign-extends directly.
            let hi = vshrq_n_s8::<4>(packed_s);
            // linterleave lo/hi so component order is lo0,hi0,lo1,hi1,...
            let zipped = vzipq_s8(lo, hi); // .0 = first 16 components
            store_s8x16_as_f32(zipped.0, &mut scratch[c * 32..]);
            store_s8x16_as_f32(zipped.1, &mut scratch[c * 32 + 16..]);
        }
        for idx in (chunks * 32)..dim {
            let byte = codes[idx / 2];
            let nib = if idx % 2 == 0 { byte } else { byte >> 4 };
            let n = nib & 0x0F;
            let s = if n & 0x08 != 0 {
                (n | 0xF0) as i8
            } else {
                n as i8
            };
            scratch[idx] = s as f32;
        }
        dot_scratch_blocked(q, &scratch, group_scales, dim, block)
    }
}

/// lWiden an i8x16 vector to four f32x4 and store into `out[..16]`.
#[target_feature(enable = "neon")]
unsafe fn store_s8x16_as_f32(v: std::arch::aarch64::int8x16_t, out: &mut [f32]) {
    unsafe {
        use std::arch::aarch64::*;
        let lo16 = vmovl_s8(vget_low_s8(v));
        let hi16 = vmovl_s8(vget_high_s8(v));
        let q0 = vcvtq_f32_s32(vmovl_s16(vget_low_s16(lo16)));
        let q1 = vcvtq_f32_s32(vmovl_s16(vget_high_s16(lo16)));
        let q2 = vcvtq_f32_s32(vmovl_s16(vget_low_s16(hi16)));
        let q3 = vcvtq_f32_s32(vmovl_s16(vget_high_s16(hi16)));
        vst1q_f32(out.as_mut_ptr(), q0);
        vst1q_f32(out.as_mut_ptr().add(4), q1);
        vst1q_f32(out.as_mut_ptr().add(8), q2);
        vst1q_f32(out.as_mut_ptr().add(12), q3);
    }
}

/// lPer-group reduction over an already-dequantized f32 scratch row,
/// matching `scalar::dot_f32_i4_blocked` operation-for-operation.
#[inline]
fn dot_scratch_blocked(
    q: &[f32],
    scratch: &[f32],
    group_scales: &[f32],
    dim: usize,
    block: usize,
) -> f32 {
    let mut acc = 0.0f32;
    let _ = dim;
    for (g, &scale) in group_scales.iter().enumerate() {
        let mut part = 0.0f32;
        let base = g * block;
        for j in 0..block {
            part += q[base + j] * scratch[base + j];
        }
        acc += part * scale;
    }
    acc
}

#[target_feature(enable = "neon")]
pub(super) unsafe fn dot_f32_i8_neon(q: &[f32], row: &[i8]) -> f32 {
    unsafe {
        use std::arch::aarch64::*;
        let dim = q.len();
        let mut acc = vdupq_n_f32(0.0);
        // lProcess 8 lanes per step (NEON's i8x8 widens cleanly to i16x8 then
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
