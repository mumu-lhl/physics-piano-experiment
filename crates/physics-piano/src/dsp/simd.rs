use num_complex::Complex;
use std::sync::OnceLock;

#[repr(C, align(32))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct F32x8(pub(crate) [f32; 8]);

#[derive(Clone, Debug, Default)]
pub(crate) struct SoaF32 {
    len: usize,
    pub(crate) blocks: Vec<F32x8>,
}

impl SoaF32 {
    pub(crate) fn zeros(len: usize) -> Self {
        Self {
            len,
            blocks: vec![F32x8::default(); len.div_ceil(8)],
        }
    }

    pub(crate) fn get(&self, index: usize) -> f32 {
        debug_assert!(index < self.len);
        self.blocks[index / 8].0[index % 8]
    }

    pub(crate) fn set(&mut self, index: usize, value: f32) {
        debug_assert!(index < self.len);
        self.blocks[index / 8].0[index % 8] = value;
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct F32TransitionSoA {
    pub(crate) p11: SoaF32,
    pub(crate) p12: SoaF32,
    pub(crate) p21: SoaF32,
    pub(crate) p22: SoaF32,
    pub(crate) g1: SoaF32,
    pub(crate) g2: SoaF32,
}

impl F32TransitionSoA {
    pub(crate) fn zeros(len: usize) -> Self {
        Self {
            p11: SoaF32::zeros(len),
            p12: SoaF32::zeros(len),
            p21: SoaF32::zeros(len),
            p22: SoaF32::zeros(len),
            g1: SoaF32::zeros(len),
            g2: SoaF32::zeros(len),
        }
    }

    pub(crate) fn set(&mut self, index: usize, phi: (f64, f64, f64, f64), gamma: (f64, f64)) {
        self.p11.set(index, phi.0 as f32);
        self.p12.set(index, phi.1 as f32);
        self.p21.set(index, phi.2 as f32);
        self.p22.set(index, phi.3 as f32);
        self.g1.set(index, gamma.0 as f32);
        self.g2.set(index, gamma.1 as f32);
    }
}

#[derive(Clone, Copy)]
enum Backend {
    Scalar,
    #[cfg(target_arch = "x86_64")]
    Avx2Fma,
    #[cfg(target_arch = "aarch64")]
    Neon,
}

static BACKEND: OnceLock<Backend> = OnceLock::new();

pub(crate) fn initialize_backend() {
    let _ = backend();
}

fn backend() -> Backend {
    *BACKEND.get_or_init(|| {
        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma") {
            return Backend::Avx2Fma;
        }
        #[cfg(target_arch = "aarch64")]
        return Backend::Neon;
        #[allow(unreachable_code)]
        Backend::Scalar
    })
}

#[derive(Clone, Copy)]
pub(crate) struct HammerProjection<'a> {
    pub(crate) force: f32,
    pub(crate) shape: &'a SoaF32,
}

pub(crate) fn update_modal_state(
    q: &mut SoaF32,
    v: &mut SoaF32,
    transition: &F32TransitionSoA,
    force: f32,
    hammer: Option<HammerProjection<'_>>,
    damper_rates: &SoaF32,
    damper_depth_dt: f32,
    damping: bool,
) {
    debug_assert_eq!(q.len(), v.len());
    debug_assert_eq!(q.len(), transition.p11.len());
    debug_assert_eq!(q.blocks.len(), transition.p11.blocks.len());
    match backend() {
        #[cfg(target_arch = "x86_64")]
        Backend::Avx2Fma => unsafe {
            update_modal_state_avx2(
                q,
                v,
                transition,
                force,
                hammer,
                damper_rates,
                damper_depth_dt,
                damping,
            )
        },
        #[cfg(target_arch = "aarch64")]
        Backend::Neon => unsafe {
            update_modal_state_neon(
                q,
                v,
                transition,
                force,
                hammer,
                damper_rates,
                damper_depth_dt,
                damping,
            )
        },
        Backend::Scalar => update_modal_state_scalar(
            q,
            v,
            transition,
            force,
            hammer,
            damper_rates,
            damper_depth_dt,
            damping,
        ),
    }
}

fn update_modal_state_scalar(
    q: &mut SoaF32,
    v: &mut SoaF32,
    transition: &F32TransitionSoA,
    force: f32,
    hammer: Option<HammerProjection<'_>>,
    damper_rates: &SoaF32,
    damper_depth_dt: f32,
    damping: bool,
) {
    for block in 0..q.blocks.len() {
        for lane in 0..8 {
            let mut f = force;
            if let Some(h) = hammer {
                f += h.force * h.shape.blocks[block].0[lane];
            }
            let old_q = q.blocks[block].0[lane];
            let old_v = v.blocks[block].0[lane];
            let mut new_q = transition.p11.blocks[block].0[lane] * old_q
                + transition.p12.blocks[block].0[lane] * old_v
                + transition.g1.blocks[block].0[lane] * f;
            let mut new_v = transition.p21.blocks[block].0[lane] * old_q
                + transition.p22.blocks[block].0[lane] * old_v
                + transition.g2.blocks[block].0[lane] * f;
            if damping {
                let x = damper_rates.blocks[block].0[lane] * damper_depth_dt;
                let factor = (1.0 - x + 0.5 * x * x).max(0.0);
                new_q *= factor;
                new_v *= factor;
            }
            q.blocks[block].0[lane] = new_q;
            v.blocks[block].0[lane] = new_v;
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn update_modal_state_avx2(
    q: &mut SoaF32,
    v: &mut SoaF32,
    transition: &F32TransitionSoA,
    force: f32,
    hammer: Option<HammerProjection<'_>>,
    damper_rates: &SoaF32,
    damper_depth_dt: f32,
    damping: bool,
) {
    use std::arch::x86_64::*;
    let force_v = _mm256_set1_ps(force);
    let hammer_v = hammer.map(|h| _mm256_set1_ps(h.force));
    let depth_dt_v = _mm256_set1_ps(damper_depth_dt);
    let one = _mm256_set1_ps(1.0);
    let half = _mm256_set1_ps(0.5);
    let zero = _mm256_setzero_ps();
    for block in 0..q.blocks.len() {
        let q_ptr = q.blocks[block].0.as_mut_ptr();
        let v_ptr = v.blocks[block].0.as_mut_ptr();
        let p11 = _mm256_load_ps(transition.p11.blocks[block].0.as_ptr());
        let p12 = _mm256_load_ps(transition.p12.blocks[block].0.as_ptr());
        let p21 = _mm256_load_ps(transition.p21.blocks[block].0.as_ptr());
        let p22 = _mm256_load_ps(transition.p22.blocks[block].0.as_ptr());
        let g1 = _mm256_load_ps(transition.g1.blocks[block].0.as_ptr());
        let g2 = _mm256_load_ps(transition.g2.blocks[block].0.as_ptr());
        let mut f = force_v;
        if let (Some(h), Some(hammer_force)) = (hammer, hammer_v) {
            let shape = _mm256_load_ps(h.shape.blocks[block].0.as_ptr());
            f = _mm256_fmadd_ps(shape, hammer_force, f);
        }
        let old_q = _mm256_load_ps(q_ptr);
        let old_v = _mm256_load_ps(v_ptr);
        let mut new_q = _mm256_fmadd_ps(p11, old_q, zero);
        new_q = _mm256_fmadd_ps(p12, old_v, new_q);
        new_q = _mm256_fmadd_ps(g1, f, new_q);
        let mut new_v = _mm256_fmadd_ps(p21, old_q, zero);
        new_v = _mm256_fmadd_ps(p22, old_v, new_v);
        new_v = _mm256_fmadd_ps(g2, f, new_v);
        if damping {
            let rates = _mm256_load_ps(damper_rates.blocks[block].0.as_ptr());
            let x = _mm256_mul_ps(rates, depth_dt_v);
            let x2 = _mm256_mul_ps(x, x);
            let factor = _mm256_fmadd_ps(half, x2, _mm256_fnmadd_ps(one, x, one));
            let factor = _mm256_max_ps(factor, zero);
            new_q = _mm256_mul_ps(new_q, factor);
            new_v = _mm256_mul_ps(new_v, factor);
        }
        _mm256_store_ps(q_ptr, new_q);
        _mm256_store_ps(v_ptr, new_v);
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn update_modal_state_neon(
    q: &mut SoaF32,
    v: &mut SoaF32,
    transition: &F32TransitionSoA,
    force: f32,
    hammer: Option<HammerProjection<'_>>,
    damper_rates: &SoaF32,
    damper_depth_dt: f32,
    damping: bool,
) {
    use std::arch::aarch64::*;
    let force_v = vdupq_n_f32(force);
    let hammer_v = hammer.map(|h| vdupq_n_f32(h.force));
    let depth_dt_v = vdupq_n_f32(damper_depth_dt);
    let one = vdupq_n_f32(1.0);
    let half = vdupq_n_f32(0.5);
    let zero = vdupq_n_f32(0.0);
    for block in 0..q.blocks.len() {
        for half_block in [0, 4] {
            let q_ptr = q.blocks[block].0.as_mut_ptr().add(half_block);
            let v_ptr = v.blocks[block].0.as_mut_ptr().add(half_block);
            let p11 = vld1q_f32(transition.p11.blocks[block].0.as_ptr().add(half_block));
            let p12 = vld1q_f32(transition.p12.blocks[block].0.as_ptr().add(half_block));
            let p21 = vld1q_f32(transition.p21.blocks[block].0.as_ptr().add(half_block));
            let p22 = vld1q_f32(transition.p22.blocks[block].0.as_ptr().add(half_block));
            let g1 = vld1q_f32(transition.g1.blocks[block].0.as_ptr().add(half_block));
            let g2 = vld1q_f32(transition.g2.blocks[block].0.as_ptr().add(half_block));
            let mut f = force_v;
            if let (Some(h), Some(hammer_force)) = (hammer, hammer_v) {
                let shape = vld1q_f32(h.shape.blocks[block].0.as_ptr().add(half_block));
                f = vfmaq_f32(f, shape, hammer_force);
            }
            let old_q = vld1q_f32(q_ptr);
            let old_v = vld1q_f32(v_ptr);
            let mut new_q = vfmaq_f32(vdupq_n_f32(0.0), p11, old_q);
            new_q = vfmaq_f32(new_q, p12, old_v);
            new_q = vfmaq_f32(new_q, g1, f);
            let mut new_v = vfmaq_f32(vdupq_n_f32(0.0), p21, old_q);
            new_v = vfmaq_f32(new_v, p22, old_v);
            new_v = vfmaq_f32(new_v, g2, f);
            if damping {
                let rates = vld1q_f32(damper_rates.blocks[block].0.as_ptr().add(half_block));
                let x = vmulq_f32(rates, depth_dt_v);
                let factor = vaddq_f32(vsubq_f32(one, x), vmulq_f32(half, vmulq_f32(x, x)));
                let factor = vmaxq_f32(factor, zero);
                new_q = vmulq_f32(new_q, factor);
                new_v = vmulq_f32(new_v, factor);
            }
            vst1q_f32(q_ptr, new_q);
            vst1q_f32(v_ptr, new_v);
        }
    }
}

pub(crate) fn complex_mac_accumulate(
    accumulator: &mut [Complex<f64>],
    input: &[Complex<f64>],
    filter: &[Complex<f64>],
) {
    debug_assert_eq!(accumulator.len(), input.len());
    debug_assert_eq!(accumulator.len(), filter.len());
    match backend() {
        #[cfg(target_arch = "x86_64")]
        Backend::Avx2Fma => unsafe {
            complex_mac_avx2(accumulator, input, filter);
        },
        #[cfg(target_arch = "aarch64")]
        Backend::Neon => unsafe {
            complex_mac_neon(accumulator, input, filter);
        },
        Backend::Scalar => {
            for ((acc, x), h) in accumulator.iter_mut().zip(input).zip(filter) {
                *acc += *x * *h;
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn complex_mac_avx2(
    accumulator: &mut [Complex<f64>],
    input: &[Complex<f64>],
    filter: &[Complex<f64>],
) {
    use std::arch::x86_64::*;
    let len = accumulator.len();
    let mut i = 0;
    let sign = _mm256_setr_pd(-1.0, 1.0, -1.0, 1.0);
    while i + 1 < len {
        let x = _mm256_loadu_pd(input.as_ptr().add(i).cast());
        let h = _mm256_loadu_pd(filter.as_ptr().add(i).cast());
        let acc = _mm256_loadu_pd(accumulator.as_ptr().add(i).cast());
        let x_re = _mm256_permute_pd(x, 0b0000);
        let x_im = _mm256_permute_pd(x, 0b1111);
        let h_swapped = _mm256_mul_pd(_mm256_permute_pd(h, 0b0101), sign);
        let out = _mm256_fmadd_pd(x_im, h_swapped, _mm256_fmadd_pd(x_re, h, acc));
        _mm256_storeu_pd(accumulator.as_mut_ptr().add(i).cast(), out);
        i += 2;
    }
    if i < len {
        accumulator[i] += input[i] * filter[i];
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn complex_mac_neon(
    accumulator: &mut [Complex<f64>],
    input: &[Complex<f64>],
    filter: &[Complex<f64>],
) {
    use std::arch::aarch64::*;
    let sign = vld1q_f64([-1.0, 1.0].as_ptr());
    for i in 0..accumulator.len() {
        let x = vld1q_f64(input.as_ptr().add(i).cast());
        let h = vld1q_f64(filter.as_ptr().add(i).cast());
        let acc = vld1q_f64(accumulator.as_ptr().add(i).cast());
        let x_re = vdupq_laneq_f64(x, 0);
        let x_im = vdupq_laneq_f64(x, 1);
        let h_swapped = vmulq_f64(vextq_f64::<1>(h, h), sign);
        let out = vfmaq_f64(vfmaq_f64(acc, x_re, h), x_im, h_swapped);
        vst1q_f64(accumulator.as_mut_ptr().add(i).cast(), out);
    }
}
