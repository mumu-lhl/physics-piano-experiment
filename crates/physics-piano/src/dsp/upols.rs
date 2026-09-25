//! Partitioned Uniform-Power Overlap-Save (UPOLS) FFT Block Convolver in Rust.

use std::f64::consts::PI;
use std::sync::Arc;
use realfft::{RealFftPlanner, RealToComplex, ComplexToReal};
use num_complex::Complex;

pub fn generate_orthotropic_soundboard_ir(
    sample_rate: f64,
    duration: f64,
    num_modes: usize,
) -> (Vec<f64>, Vec<f64>) {
    let num_samples = (duration * sample_rate) as usize;
    let mut ir_left = vec![0.0; num_samples];
    let mut ir_right = vec![0.0; num_samples];

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
        let phi_bass = (m * PI * 0.42).sin() * (n * PI * 0.72).sin();
        let phi_treble = (m * PI * 0.58).sin() * (n * PI * 0.35).sin();
        let damping = 2.5 + 0.0035 * freq + 2.5e-7 * freq.powi(2);
        let gain = 1.0 / (freq.max(40.0)).sqrt();

        let phase_l = (m * 0.43 + n * 0.81) % (2.0 * PI);
        let phase_r = (m * 0.77 + n * 0.29) % (2.0 * PI);
        let omega = 2.0 * PI * freq;

        for s in 0..num_samples {
            let t = s as f64 / sample_rate;
            let env = (-damping * t).exp();
            ir_left[s] += phi_bass * gain * env * (omega * t + phase_l).sin();
            ir_right[s] += phi_treble * gain * env * (omega * t + phase_r).sin();
        }
    }

    // Normalization
    let mut peak = 1e-6f64;
    for s in 0..num_samples {
        peak = peak.max(ir_left[s].abs()).max(ir_right[s].abs());
    }
    for s in 0..num_samples {
        ir_left[s] /= peak;
        ir_right[s] /= peak;
    }

    (ir_left, ir_right)
}

pub struct UPOLSConvolver {
    pub block_size: usize,
    pub fft_size: usize,
    pub num_parts: usize,

    // Precomputed partition frequency spectra: [num_parts][2][rfft_bins]
    pub h_left: Vec<Vec<Complex<f64>>>,
    pub h_right: Vec<Vec<Complex<f64>>>,

    // Forward and inverse FFT processors
    r2c: Arc<dyn RealToComplex<f64>>,
    c2r: Arc<dyn ComplexToReal<f64>>,

    // History and delay buffers
    prev_x: Vec<f64>,
    x_history: Vec<Vec<Complex<f64>>>,
    history_idx: usize,

    // Scratch buffers to avoid allocation in audio loop
    time_frame: Vec<f64>,
    freq_scratch: Vec<Complex<f64>>,
    y_freq_left: Vec<Complex<f64>>,
    y_freq_right: Vec<Complex<f64>>,
    y_time_scratch: Vec<f64>,

    // Sample streaming ring buffers
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

        // 1. Construct 2B frame: [prev_x, input]
        self.time_frame[..b].copy_from_slice(&self.prev_x);
        self.time_frame[b..].copy_from_slice(input);
        self.prev_x.copy_from_slice(input);

        // 2. Forward FFT
        self.r2c.process(&mut self.time_frame, &mut self.freq_scratch).unwrap();

        // 3. Store in circular history
        self.x_history[self.history_idx].copy_from_slice(&self.freq_scratch);

        // 4. Frequency domain MAC
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

        // 5. Inverse FFT and extract second half (Overlap-Save)
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
            let mut out_l_block = vec![0.0; self.block_size];
            let mut out_r_block = vec![0.0; self.block_size];
            self.process_block(&self.in_fifo.clone(), &mut out_l_block, &mut out_r_block);
            self.out_fifo_left.copy_from_slice(&out_l_block);
            self.out_fifo_right.copy_from_slice(&out_r_block);
            self.fifo_idx = 0;
        }

        (out_l, out_r)
    }
}
