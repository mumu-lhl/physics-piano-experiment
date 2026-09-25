//! Partitioned Uniform-Power Overlap-Save (UPOLS) FFT Block Convolver in Rust (Tier 7).
//!
//! Features:
//! - Multi-channel / multi-perspective spatial radiation (Close, Player, Ambient)
//! - Shared forward FFT across all perspectives for minimal CPU overhead
//! - Zero-allocation sample-by-sample and block-by-block streaming
//! - Orthotropic spruce soundboard analytical multi-perspective IR generator

use std::f64::consts::PI;
use std::sync::Arc;
use realfft::{RealFftPlanner, RealToComplex, ComplexToReal};
use num_complex::Complex;

#[derive(Debug, Clone)]
pub struct StereoIR {
    pub left: Vec<f64>,
    pub right: Vec<f64>,
}

/// Generates calibrated 3-perspective soundboard impulse responses:
/// 1. Close: Near hammer rail, high transient clarity, wide stereo imaging.
/// 2. Player: Seated binaural perspective, balanced body resonance.
/// 3. Ambient: Hall / Decca tree perspective, diffuse modal tail.
pub fn generate_multi_perspective_soundboard_irs(
    sample_rate: f64,
    duration: f64,
    num_modes: usize,
) -> (StereoIR, StereoIR, StereoIR) {
    let num_samples = (duration * sample_rate) as usize;
    let mut close_l = vec![0.0; num_samples];
    let mut close_r = vec![0.0; num_samples];
    let mut player_l = vec![0.0; num_samples];
    let mut player_r = vec![0.0; num_samples];
    let mut ambient_l = vec![0.0; num_samples];
    let mut ambient_r = vec![0.0; num_samples];

    let lx = 2.1f64;
    let ly = 1.5f64;
    let rho_wood = 430.0f64;
    let h_plate: f64 = 0.0095;
    let ex = 1.1e10f64;
    let ey = 6.8e8f64;
    let nu: f64 = 0.38;
    let gxy = 7.5e8f64;

    let dx = (ex * h_plate.powi(3)) / (12.0 * (1.0 - nu.powi(2)));
    let dy = (ey * h_plate.powi(3)) / (12.0 * (1.0 - nu.powi(2)));
    let dxy = (gxy * h_plate.powi(3)) / 12.0;

    let mut modes = Vec::new();
    for m in 1..24 {
        let kx = m as f64 * PI / lx;
        for n in 1..16 {
            let ky = n as f64 * PI / ly;
            let omega_sq = (dx * kx.powi(4) + 2.0 * dxy * kx.powi(2) * ky.powi(2) + dy * ky.powi(4))
                / (rho_wood * h_plate);
            let omega = omega_sq.sqrt();
            let freq = omega / (2.0 * PI);
            if (30.0..=12000.0).contains(&freq) {
                modes.push((freq, m as f64, n as f64));
            }
        }
    }
    modes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    modes.truncate(num_modes);

    for (freq, m, n) in modes {
        let omega = 2.0 * PI * freq;
        let base_gain = 1.0 / (freq.max(40.0)).sqrt();

        // 1. Close perspective: rapid initial transient, wide stereo separation
        let phi_close_l = (m * PI * 0.38).sin() * (n * PI * 0.80).sin();
        let phi_close_r = (m * PI * 0.62).sin() * (n * PI * 0.28).sin();
        let damp_close = 3.5 + 0.0045 * freq + 3.0e-7 * freq.powi(2);
        let phase_close_l = (m * 0.52 + n * 0.74) % (2.0 * PI);
        let phase_close_r = (m * 0.81 + n * 0.33) % (2.0 * PI);

        // 2. Player perspective: seated distance (~1.2m), HRTF interaural phase delay
        let phi_player_l = (m * PI * 0.44).sin() * (n * PI * 0.65).sin();
        let phi_player_r = (m * PI * 0.56).sin() * (n * PI * 0.45).sin();
        let damp_player = 2.8 + 0.0035 * freq + 2.2e-7 * freq.powi(2);
        let phase_player_l = (m * 0.41 + n * 0.62) % (2.0 * PI);
        let phase_player_r = (phase_player_l + 0.00045 * freq) % (2.0 * PI); // ~0.45ms head delay

        // 3. Ambient perspective: diffuse hall radiation, rich modal tail
        let phi_ambient_l = (m * PI * 0.48).sin() * (n * PI * 0.52).sin();
        let phi_ambient_r = (m * PI * 0.52).sin() * (n * PI * 0.48).sin();
        let damp_ambient = 1.9 + 0.0022 * freq + 1.5e-7 * freq.powi(2);
        let phase_amb_l = (m * 0.93 + n * 0.17) % (2.0 * PI);
        let phase_amb_r = (m * 0.29 + n * 0.85) % (2.0 * PI);

        for s in 0..num_samples {
            let t = s as f64 / sample_rate;

            let env_c = (-damp_close * t).exp();
            close_l[s] += phi_close_l * base_gain * env_c * (omega * t + phase_close_l).sin();
            close_r[s] += phi_close_r * base_gain * env_c * (omega * t + phase_close_r).sin();

            let env_p = (-damp_player * t).exp();
            player_l[s] += phi_player_l * base_gain * env_p * (omega * t + phase_player_l).sin();
            player_r[s] += phi_player_r * base_gain * env_p * (omega * t + phase_player_r).sin();

            let env_a = (-damp_ambient * t).exp();
            ambient_l[s] += phi_ambient_l * base_gain * env_a * (omega * t + phase_amb_l).sin();
            ambient_r[s] += phi_ambient_r * base_gain * env_a * (omega * t + phase_amb_r).sin();
        }
    }

    // Normalize each perspective and apply bridge-force to sound pressure radiation scale (~1e-4).
    // Bridge dynamic force from string vibration is ~50-200 Newtons. The acoustic transfer function
    // from bridge force to air pressure at 1 meter scales to [0.0, 1.0] audio range via ~0.7e-4.
    let normalize_and_scale = |ir_l: &mut [f64], ir_r: &mut [f64], scale: f64| {
        let mut peak = 1e-6f64;
        for s in 0..ir_l.len() {
            peak = peak.max(ir_l[s].abs()).max(ir_r[s].abs());
        }
        for s in 0..ir_l.len() {
            ir_l[s] = (ir_l[s] / peak) * scale;
            ir_r[s] = (ir_r[s] / peak) * scale;
        }
    };

    normalize_and_scale(&mut close_l, &mut close_r, 0.7e-4);
    normalize_and_scale(&mut player_l, &mut player_r, 0.5e-4);
    normalize_and_scale(&mut ambient_l, &mut ambient_r, 0.35e-4);

    (
        StereoIR { left: close_l, right: close_r },
        StereoIR { left: player_l, right: player_r },
        StereoIR { left: ambient_l, right: ambient_r },
    )
}

pub fn generate_orthotropic_soundboard_ir(
    sample_rate: f64,
    duration: f64,
    num_modes: usize,
) -> (Vec<f64>, Vec<f64>) {
    let (c, _, _) = generate_multi_perspective_soundboard_irs(sample_rate, duration, num_modes);
    (c.left, c.right)
}

/// Zero-Allocation Multi-Perspective UPOLS Convolver with Shared Forward-FFT.
pub struct MultiPerspectiveUPOLS {
    pub block_size: usize,
    pub fft_size: usize,
    pub num_parts: usize,

    // Partition spectra: [perspective (0=Close, 1=Player, 2=Ambient)][part][bin]
    pub h_left: [Vec<Vec<Complex<f64>>>; 3],
    pub h_right: [Vec<Vec<Complex<f64>>>; 3],

    r2c: Arc<dyn RealToComplex<f64>>,
    c2r: Arc<dyn ComplexToReal<f64>>,

    prev_x: Vec<f64>,
    x_history: Vec<Vec<Complex<f64>>>,
    history_idx: usize,

    // Scratch buffers for zero allocation
    time_frame: Vec<f64>,
    freq_scratch: Vec<Complex<f64>>,
    y_freq_left: [Vec<Complex<f64>>; 3],
    y_freq_right: [Vec<Complex<f64>>; 3],
    y_time_scratch: Vec<f64>,

    // Block streaming FIFO
    in_fifo: Vec<f64>,
    out_fifo_left: Vec<f64>,
    out_fifo_right: Vec<f64>,
    fifo_idx: usize,

    // Mix fader gains
    pub close_gain: f64,
    pub player_gain: f64,
    pub ambient_gain: f64,
}

impl MultiPerspectiveUPOLS {
    pub fn new(
        close: &StereoIR,
        player: &StereoIR,
        ambient: &StereoIR,
        block_size: usize,
    ) -> Self {
        let b = block_size;
        let fft_size = 2 * b;
        let rfft_bins = fft_size / 2 + 1;

        let ir_len = close.left.len()
            .max(close.right.len())
            .max(player.left.len())
            .max(ambient.left.len());
        let num_parts = (ir_len + b - 1) / b;

        let mut planner = RealFftPlanner::<f64>::new();
        let r2c = planner.plan_fft_forward(fft_size);
        let c2r = planner.plan_fft_inverse(fft_size);

        let irs = [close, player, ambient];
        let mut h_left: [Vec<Vec<Complex<f64>>>; 3] = [
            Vec::with_capacity(num_parts),
            Vec::with_capacity(num_parts),
            Vec::with_capacity(num_parts),
        ];
        let mut h_right: [Vec<Vec<Complex<f64>>>; 3] = [
            Vec::with_capacity(num_parts),
            Vec::with_capacity(num_parts),
            Vec::with_capacity(num_parts),
        ];

        let mut padded = vec![0.0; fft_size];
        let mut spectrum = vec![Complex::new(0.0, 0.0); rfft_bins];

        for m in 0..3 {
            let ir = irs[m];
            for p in 0..num_parts {
                let start = p * b;

                // Left channel partition
                let end_l = (start + b).min(ir.left.len());
                padded.fill(0.0);
                if start < ir.left.len() {
                    padded[..(end_l - start)].copy_from_slice(&ir.left[start..end_l]);
                }
                r2c.process(&mut padded, &mut spectrum).unwrap();
                h_left[m].push(spectrum.clone());

                // Right channel partition
                let end_r = (start + b).min(ir.right.len());
                padded.fill(0.0);
                if start < ir.right.len() {
                    padded[..(end_r - start)].copy_from_slice(&ir.right[start..end_r]);
                }
                r2c.process(&mut padded, &mut spectrum).unwrap();
                h_right[m].push(spectrum.clone());
            }
        }

        Self {
            block_size: b,
            fft_size,
            num_parts,
            h_left,
            h_right,
            r2c,
            c2r,
            prev_x: vec![0.0; b],
            x_history: vec![vec![Complex::new(0.0, 0.0); rfft_bins]; num_parts],
            history_idx: 0,
            time_frame: vec![0.0; fft_size],
            freq_scratch: vec![Complex::new(0.0, 0.0); rfft_bins],
            y_freq_left: [
                vec![Complex::new(0.0, 0.0); rfft_bins],
                vec![Complex::new(0.0, 0.0); rfft_bins],
                vec![Complex::new(0.0, 0.0); rfft_bins],
            ],
            y_freq_right: [
                vec![Complex::new(0.0, 0.0); rfft_bins],
                vec![Complex::new(0.0, 0.0); rfft_bins],
                vec![Complex::new(0.0, 0.0); rfft_bins],
            ],
            y_time_scratch: vec![0.0; fft_size],
            in_fifo: vec![0.0; b],
            out_fifo_left: vec![0.0; b],
            out_fifo_right: vec![0.0; b],
            fifo_idx: 0,
            close_gain: 1.0,
            player_gain: 0.707, // -3dB
            ambient_gain: 0.501, // -6dB
        }
    }

    /// Process one audio block with shared forward FFT across all 3 perspectives.
    pub fn process_block(&mut self, input: &[f64], out_left: &mut [f64], out_right: &mut [f64]) {
        let b = self.block_size;
        assert_eq!(input.len(), b);
        assert_eq!(out_left.len(), b);
        assert_eq!(out_right.len(), b);

        // 1. Construct 2B frame: [prev_x, input]
        self.time_frame[..b].copy_from_slice(&self.prev_x);
        self.time_frame[b..].copy_from_slice(input);
        self.prev_x.copy_from_slice(input);

        // 2. Shared single forward FFT
        self.r2c.process(&mut self.time_frame, &mut self.freq_scratch).unwrap();

        // 3. Store in circular history
        self.x_history[self.history_idx].copy_from_slice(&self.freq_scratch);

        // 4. Frequency-domain MAC for all perspectives
        for m in 0..3 {
            self.y_freq_left[m].fill(Complex::new(0.0, 0.0));
            self.y_freq_right[m].fill(Complex::new(0.0, 0.0));
        }

        let num_bins = self.fft_size / 2 + 1;
        for p in 0..self.num_parts {
            let idx = (self.history_idx + self.num_parts - p) % self.num_parts;
            let x_p = &self.x_history[idx];

            for m in 0..3 {
                let hl = &self.h_left[m][p];
                let hr = &self.h_right[m][p];
                let yl = &mut self.y_freq_left[m];
                let yr = &mut self.y_freq_right[m];

                for k in 0..num_bins {
                    yl[k] += x_p[k] * hl[k];
                    yr[k] += x_p[k] * hr[k];
                }
            }
        }

        self.history_idx = (self.history_idx + 1) % self.num_parts;

        // 5. Inverse FFT and mixdown into out_left and out_right
        out_left.fill(0.0);
        out_right.fill(0.0);

        let norm = 1.0 / self.fft_size as f64;
        let gains = [self.close_gain, self.player_gain, self.ambient_gain];

        for m in 0..3 {
            let g = gains[m];
            if g <= 1e-5 {
                continue;
            }

            // Left
            self.c2r.process(&mut self.y_freq_left[m], &mut self.y_time_scratch).unwrap();
            for i in 0..b {
                out_left[i] += self.y_time_scratch[b + i] * norm * g;
            }

            // Right
            self.c2r.process(&mut self.y_freq_right[m], &mut self.y_time_scratch).unwrap();
            for i in 0..b {
                out_right[i] += self.y_time_scratch[b + i] * norm * g;
            }
        }
    }

    /// Process a single audio sample (zero allocations).
    #[inline]
    pub fn process_sample(&mut self, sample_in: f64) -> (f64, f64) {
        self.in_fifo[self.fifo_idx] = sample_in;
        let out_l = self.out_fifo_left[self.fifo_idx];
        let out_r = self.out_fifo_right[self.fifo_idx];
        self.fifo_idx += 1;

        if self.fifo_idx >= self.block_size {
            let b = self.block_size;
            self.time_frame[..b].copy_from_slice(&self.prev_x);
            self.time_frame[b..].copy_from_slice(&self.in_fifo);
            self.prev_x.copy_from_slice(&self.in_fifo);

            self.r2c.process(&mut self.time_frame, &mut self.freq_scratch).unwrap();
            self.x_history[self.history_idx].copy_from_slice(&self.freq_scratch);

            for m in 0..3 {
                self.y_freq_left[m].fill(Complex::new(0.0, 0.0));
                self.y_freq_right[m].fill(Complex::new(0.0, 0.0));
            }

            let num_bins = self.fft_size / 2 + 1;
            for p in 0..self.num_parts {
                let idx = (self.history_idx + self.num_parts - p) % self.num_parts;
                let x_p = &self.x_history[idx];

                for m in 0..3 {
                    let hl = &self.h_left[m][p];
                    let hr = &self.h_right[m][p];
                    let yl = &mut self.y_freq_left[m];
                    let yr = &mut self.y_freq_right[m];

                    for k in 0..num_bins {
                        yl[k] += x_p[k] * hl[k];
                        yr[k] += x_p[k] * hr[k];
                    }
                }
            }

            self.history_idx = (self.history_idx + 1) % self.num_parts;

            self.out_fifo_left.fill(0.0);
            self.out_fifo_right.fill(0.0);

            let norm = 1.0 / self.fft_size as f64;
            let gains = [self.close_gain, self.player_gain, self.ambient_gain];

            for m in 0..3 {
                let g = gains[m];
                if g <= 1e-5 {
                    continue;
                }

                self.c2r.process(&mut self.y_freq_left[m], &mut self.y_time_scratch).unwrap();
                for i in 0..b {
                    self.out_fifo_left[i] += self.y_time_scratch[b + i] * norm * g;
                }

                self.c2r.process(&mut self.y_freq_right[m], &mut self.y_time_scratch).unwrap();
                for i in 0..b {
                    self.out_fifo_right[i] += self.y_time_scratch[b + i] * norm * g;
                }
            }

            self.fifo_idx = 0;
        }

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        self.prev_x.fill(0.0);
        for h in &mut self.x_history {
            h.fill(Complex::new(0.0, 0.0));
        }
        self.history_idx = 0;
        self.in_fifo.fill(0.0);
        self.out_fifo_left.fill(0.0);
        self.out_fifo_right.fill(0.0);
        self.fifo_idx = 0;
    }
}

/// Legacy single stereo UPOLS Convolver (optimized for zero allocation).
pub struct UPOLSConvolver {
    pub block_size: usize,
    pub fft_size: usize,
    pub num_parts: usize,

    pub h_left: Vec<Vec<Complex<f64>>>,
    pub h_right: Vec<Vec<Complex<f64>>>,

    r2c: Arc<dyn RealToComplex<f64>>,
    c2r: Arc<dyn ComplexToReal<f64>>,

    prev_x: Vec<f64>,
    x_history: Vec<Vec<Complex<f64>>>,
    history_idx: usize,

    time_frame: Vec<f64>,
    freq_scratch: Vec<Complex<f64>>,
    y_freq_left: Vec<Complex<f64>>,
    y_freq_right: Vec<Complex<f64>>,
    y_time_scratch: Vec<f64>,

    in_fifo: Vec<f64>,
    out_fifo_left: Vec<f64>,
    out_fifo_right: Vec<f64>,
    fifo_idx: usize,
}

impl UPOLSConvolver {
    pub fn new(ir_left: &[f64], ir_right: &[f64], block_size: usize) -> Self {
        let b = block_size;
        let fft_size = 2 * b;
        let rfft_bins = fft_size / 2 + 1;

        let ir_len = ir_left.len().max(ir_right.len());
        let num_parts = (ir_len + b - 1) / b;

        let mut planner = RealFftPlanner::<f64>::new();
        let r2c = planner.plan_fft_forward(fft_size);
        let c2r = planner.plan_fft_inverse(fft_size);

        let mut h_left = Vec::with_capacity(num_parts);
        let mut h_right = Vec::with_capacity(num_parts);

        let mut padded = vec![0.0; fft_size];
        let mut spectrum = vec![Complex::new(0.0, 0.0); rfft_bins];

        for p in 0..num_parts {
            let start = p * b;
            let end = (start + b).min(ir_left.len());
            padded.fill(0.0);
            if start < ir_left.len() {
                padded[..(end - start)].copy_from_slice(&ir_left[start..end]);
            }
            r2c.process(&mut padded, &mut spectrum).unwrap();
            h_left.push(spectrum.clone());

            let end_r = (start + b).min(ir_right.len());
            padded.fill(0.0);
            if start < ir_right.len() {
                padded[..(end_r - start)].copy_from_slice(&ir_right[start..end_r]);
            }
            r2c.process(&mut padded, &mut spectrum).unwrap();
            h_right.push(spectrum.clone());
        }

        Self {
            block_size: b,
            fft_size,
            num_parts,
            h_left,
            h_right,
            r2c,
            c2r,
            prev_x: vec![0.0; b],
            x_history: vec![vec![Complex::new(0.0, 0.0); rfft_bins]; num_parts],
            history_idx: 0,
            time_frame: vec![0.0; fft_size],
            freq_scratch: vec![Complex::new(0.0, 0.0); rfft_bins],
            y_freq_left: vec![Complex::new(0.0, 0.0); rfft_bins],
            y_freq_right: vec![Complex::new(0.0, 0.0); rfft_bins],
            y_time_scratch: vec![0.0; fft_size],
            in_fifo: vec![0.0; b],
            out_fifo_left: vec![0.0; b],
            out_fifo_right: vec![0.0; b],
            fifo_idx: 0,
        }
    }

    pub fn process_block(&mut self, input: &[f64], out_left: &mut [f64], out_right: &mut [f64]) {
        let b = self.block_size;
        assert_eq!(input.len(), b);
        assert_eq!(out_left.len(), b);
        assert_eq!(out_right.len(), b);

        self.time_frame[..b].copy_from_slice(&self.prev_x);
        self.time_frame[b..].copy_from_slice(input);
        self.prev_x.copy_from_slice(input);

        self.r2c.process(&mut self.time_frame, &mut self.freq_scratch).unwrap();
        self.x_history[self.history_idx].copy_from_slice(&self.freq_scratch);

        self.y_freq_left.fill(Complex::new(0.0, 0.0));
        self.y_freq_right.fill(Complex::new(0.0, 0.0));

        let num_bins = self.fft_size / 2 + 1;
        for p in 0..self.num_parts {
            let idx = (self.history_idx + self.num_parts - p) % self.num_parts;
            let x_p = &self.x_history[idx];
            let hl = &self.h_left[p];
            let hr = &self.h_right[p];

            for k in 0..num_bins {
                self.y_freq_left[k] += x_p[k] * hl[k];
                self.y_freq_right[k] += x_p[k] * hr[k];
            }
        }

        self.history_idx = (self.history_idx + 1) % self.num_parts;

        let norm = 1.0 / self.fft_size as f64;

        self.c2r.process(&mut self.y_freq_left, &mut self.y_time_scratch).unwrap();
        for i in 0..b {
            out_left[i] = self.y_time_scratch[b + i] * norm;
        }

        self.c2r.process(&mut self.y_freq_right, &mut self.y_time_scratch).unwrap();
        for i in 0..b {
            out_right[i] = self.y_time_scratch[b + i] * norm;
        }
    }

    #[inline]
    pub fn process_sample(&mut self, sample_in: f64) -> (f64, f64) {
        self.in_fifo[self.fifo_idx] = sample_in;
        let out_l = self.out_fifo_left[self.fifo_idx];
        let out_r = self.out_fifo_right[self.fifo_idx];
        self.fifo_idx += 1;

        if self.fifo_idx >= self.block_size {
            let b = self.block_size;
            self.time_frame[..b].copy_from_slice(&self.prev_x);
            self.time_frame[b..].copy_from_slice(&self.in_fifo);
            self.prev_x.copy_from_slice(&self.in_fifo);

            self.r2c.process(&mut self.time_frame, &mut self.freq_scratch).unwrap();
            self.x_history[self.history_idx].copy_from_slice(&self.freq_scratch);

            self.y_freq_left.fill(Complex::new(0.0, 0.0));
            self.y_freq_right.fill(Complex::new(0.0, 0.0));

            let num_bins = self.fft_size / 2 + 1;
            for p in 0..self.num_parts {
                let idx = (self.history_idx + self.num_parts - p) % self.num_parts;
                let x_p = &self.x_history[idx];
                let hl = &self.h_left[p];
                let hr = &self.h_right[p];

                for k in 0..num_bins {
                    self.y_freq_left[k] += x_p[k] * hl[k];
                    self.y_freq_right[k] += x_p[k] * hr[k];
                }
            }

            self.history_idx = (self.history_idx + 1) % self.num_parts;

            let norm = 1.0 / self.fft_size as f64;

            self.c2r.process(&mut self.y_freq_left, &mut self.y_time_scratch).unwrap();
            for i in 0..b {
                self.out_fifo_left[i] = self.y_time_scratch[b + i] * norm;
            }

            self.c2r.process(&mut self.y_freq_right, &mut self.y_time_scratch).unwrap();
            for i in 0..b {
                self.out_fifo_right[i] = self.y_time_scratch[b + i] * norm;
            }

            self.fifo_idx = 0;
        }

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        self.prev_x.fill(0.0);
        for h in &mut self.x_history {
            h.fill(Complex::new(0.0, 0.0));
        }
        self.history_idx = 0;
        self.in_fifo.fill(0.0);
        self.out_fifo_left.fill(0.0);
        self.out_fifo_right.fill(0.0);
        self.fifo_idx = 0;
    }
}
