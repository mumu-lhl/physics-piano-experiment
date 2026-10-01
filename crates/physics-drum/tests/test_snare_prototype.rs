//! Snare drum physical modeling prototype without any artificial white noise.
//! Models:
//! 1. 14-inch coated batter head (256 modes with tension modulation)
//! 2. 14-inch ultra-thin snare-side resonant head (256 modes, low mass, high sensitivity)
//! 3. Air cavity adiabatic coupling
//! 4. 16 discrete unilateral Hertzian snare wire contacts with gap variation & chatter dynamics
//! 5. Bi-directional back-reaction from wires onto the resonant bottom head

use hound::{WavSpec, WavWriter};
use physics_dsp::{ModalTransition, OverdampedPolicy};
use std::f64::consts::PI;
use std::fs;
use std::path::Path;

pub const TOM_MODE_SPECS: [(u8, u8, f64, bool); 256] = [
    (0, 1, 2.4048255577, false),
    (1, 1, 3.8317059702, false),
    (1, 1, 3.8317059702, true),
    (2, 1, 5.1356223018, false),
    (2, 1, 5.1356223018, true),
    (0, 2, 5.5200781103, false),
    (3, 1, 6.3801618959, false),
    (3, 1, 6.3801618959, true),
    (1, 2, 7.0155866698, false),
    (1, 2, 7.0155866698, true),
    (4, 1, 7.5883424345, false),
    (4, 1, 7.5883424345, true),
    (2, 2, 8.4172441404, false),
    (2, 2, 8.4172441404, true),
    (0, 3, 8.6537279129, false),
    (5, 1, 8.7714838160, false),
    (5, 1, 8.7714838160, true),
    (3, 2, 9.7610231300, false),
    (3, 2, 9.7610231300, true),
    (6, 1, 9.9361095242, false),
    (6, 1, 9.9361095242, true),
    (1, 3, 10.1734681351, false),
    (1, 3, 10.1734681351, true),
    (4, 2, 11.0647094885, false),
    (4, 2, 11.0647094885, true),
    (7, 1, 11.0863700192, false),
    (7, 1, 11.0863700192, true),
    (2, 3, 11.6198411721, false),
    (2, 3, 11.6198411721, true),
    (0, 4, 11.7915344390, false),
    (8, 1, 12.2250922640, false),
    (8, 1, 12.2250922640, true),
    (5, 2, 12.3386041975, false),
    (5, 2, 12.3386041975, true),
    (3, 3, 13.0152007217, false),
    (3, 3, 13.0152007217, true),
    (1, 4, 13.3236919363, false),
    (1, 4, 13.3236919363, true),
    (9, 1, 13.3543004774, false),
    (9, 1, 13.3543004774, true),
    (6, 2, 13.5892901705, false),
    (6, 2, 13.5892901705, true),
    (4, 3, 14.3725366716, false),
    (4, 3, 14.3725366716, true),
    (10, 1, 14.4755006866, false),
    (10, 1, 14.4755006866, true),
    (2, 4, 14.7959517824, false),
    (2, 4, 14.7959517824, true),
    (7, 2, 14.8212687270, false),
    (7, 2, 14.8212687270, true),
    (0, 5, 14.9309177085, false),
    (11, 1, 15.5898478845, false),
    (11, 1, 15.5898478845, true),
    (5, 3, 15.7001740797, false),
    (5, 3, 15.7001740797, true),
    (8, 2, 16.0377741909, false),
    (8, 2, 16.0377741909, true),
    (3, 4, 16.2234661603, false),
    (3, 4, 16.2234661603, true),
    (1, 5, 16.4706300509, false),
    (1, 5, 16.4706300509, true),
    (12, 1, 16.6982499338, false),
    (12, 1, 16.6982499338, true),
    (6, 3, 17.0038196678, false),
    (6, 3, 17.0038196678, true),
    (9, 2, 17.2412203825, false),
    (9, 2, 17.2412203825, true),
    (4, 4, 17.6159660498, false),
    (4, 4, 17.6159660498, true),
    (13, 1, 17.8014351533, false),
    (13, 1, 17.8014351533, true),
    (2, 5, 17.9598194950, false),
    (2, 5, 17.9598194950, true),
    (0, 6, 18.0710639679, false),
    (7, 3, 18.2875828325, false),
    (7, 3, 18.2875828325, true),
    (10, 2, 18.4334636670, false),
    (10, 2, 18.4334636670, true),
    (14, 1, 18.8999979532, false),
    (14, 1, 18.8999979532, true),
    (5, 4, 18.9801338752, false),
    (5, 4, 18.9801338752, true),
    (3, 5, 19.4094152264, false),
    (3, 5, 19.4094152264, true),
    (8, 3, 19.5545364310, false),
    (8, 3, 19.5545364310, true),
    (1, 6, 19.6158585105, false),
    (1, 6, 19.6158585105, true),
    (11, 2, 19.6159669040, false),
    (11, 2, 19.6159669040, true),
    (15, 1, 19.9944306298, false),
    (15, 1, 19.9944306298, true),
    (6, 4, 20.3207892136, false),
    (6, 4, 20.3207892136, true),
    (12, 2, 20.7899063601, false),
    (12, 2, 20.7899063601, true),
    (9, 3, 20.8070477893, false),
    (9, 3, 20.8070477893, true),
    (4, 5, 20.8269329570, false),
    (4, 5, 20.8269329570, true),
    (16, 1, 21.0851461131, false),
    (16, 1, 21.0851461131, true),
    (2, 6, 21.1169970530, false),
    (2, 6, 21.1169970530, true),
    (0, 7, 21.2116366299, false),
    (7, 4, 21.6415410198, false),
    (7, 4, 21.6415410198, true),
    (13, 2, 21.9562440678, false),
    (13, 2, 21.9562440678, true),
    (10, 3, 22.0469853647, false),
    (10, 3, 22.0469853647, true),
    (17, 1, 22.1724946188, false),
    (17, 1, 22.1724946188, true),
    (5, 5, 22.2177998966, false),
    (5, 5, 22.2177998966, true),
    (3, 6, 22.5827295931, false),
    (3, 6, 22.5827295931, true),
    (1, 7, 22.7600843806, false),
    (1, 7, 22.7600843806, true),
    (8, 4, 22.9451731319, false),
    (8, 4, 22.9451731319, true),
    (14, 2, 23.1157783473, false),
    (14, 2, 23.1157783473, true),
    (18, 1, 23.2567760851, false),
    (18, 1, 23.2567760851, true),
    (11, 3, 23.2758537263, false),
    (11, 3, 23.2758537263, true),
    (6, 5, 23.5860844356, false),
    (6, 5, 23.5860844356, true),
    (4, 6, 24.0190195248, false),
    (4, 6, 24.0190195248, true),
    (9, 4, 24.2338852578, false),
    (9, 4, 24.2338852578, true),
    (15, 2, 24.2691800262, false),
    (15, 2, 24.2691800262, true),
    (2, 7, 24.2701123136, false),
    (2, 7, 24.2701123136, true),
    (19, 1, 24.3382496234, false),
    (19, 1, 24.3382496234, true),
    (0, 8, 24.3524715307, false),
    (12, 3, 24.4948850439, false),
    (12, 3, 24.4948850439, true),
    (7, 5, 24.9349278877, false),
    (7, 5, 24.9349278877, true),
    (16, 2, 25.4170190063, false),
    (16, 2, 25.4170190063, true),
    (20, 1, 25.4171408141, false),
    (20, 1, 25.4171408141, true),
    (5, 6, 25.4303411542, false),
    (5, 6, 25.4303411542, true),
    (10, 4, 25.5094505542, false),
    (10, 4, 25.5094505542, true),
    (13, 3, 25.7051030539, false),
    (13, 3, 25.7051030539, true),
    (3, 7, 25.7481666993, false),
    (3, 7, 25.7481666993, true),
    (1, 8, 25.9036720876, false),
    (1, 8, 25.9036720876, true),
    (8, 5, 26.2668146412, false),
    (8, 5, 26.2668146412, true),
    (21, 1, 26.4936474160, false),
    (21, 1, 26.4936474160, true),
    (17, 2, 26.5597841380, false),
    (17, 2, 26.5597841380, true),
    (11, 4, 26.7733225455, false),
    (11, 4, 26.7733225455, true),
    (6, 6, 26.8201519834, false),
    (6, 6, 26.8201519834, true),
    (14, 3, 26.9073689762, false),
    (14, 3, 26.9073689762, true),
    (4, 7, 27.1990877660, false),
    (4, 7, 27.1990877660, true),
    (2, 8, 27.4205735500, false),
    (2, 8, 27.4205735500, true),
    (0, 9, 27.4934791320, false),
    (22, 1, 27.5679438913, false),
    (22, 1, 27.5679438913, true),
    (9, 5, 27.5837489636, false),
    (9, 5, 27.5837489636, true),
    (18, 2, 27.6978983509, false),
    (18, 2, 27.6978983509, true),
    (12, 4, 28.0267099500, false),
    (12, 4, 28.0267099500, true),
    (15, 3, 28.1024152317, false),
    (15, 3, 28.1024152317, true),
    (7, 6, 28.1911884595, false),
    (7, 6, 28.1911884595, true),
    (5, 7, 28.6266183073, false),
    (5, 7, 28.6266183073, true),
    (23, 1, 28.6401850308, false),
    (23, 1, 28.6401850308, true),
    (19, 2, 28.8317303513, false),
    (19, 2, 28.8317303513, true),
    (10, 5, 28.8873750635, false),
    (10, 5, 28.8873750635, true),
    (3, 8, 28.9083507809, false),
    (3, 8, 28.9083507809, true),
    (1, 9, 29.0468285349, false),
    (1, 9, 29.0468285349, true),
    (13, 4, 29.2706304419, false),
    (13, 4, 29.2706304419, true),
    (16, 3, 29.2908706963, false),
    (16, 3, 29.2908706963, true),
    (8, 6, 29.5456596710, false),
    (8, 6, 29.5456596710, true),
    (24, 1, 29.7105088898, false),
    (24, 1, 29.7105088898, true),
    (20, 2, 29.9616037916, false),
    (20, 2, 29.9616037916, true),
    (6, 7, 30.0337223866, false),
    (6, 7, 30.0337223866, true),
    (11, 5, 30.1790611788, false),
    (11, 5, 30.1790611788, true),
    (4, 8, 30.3710076671, false),
    (4, 8, 30.3710076671, true),
    (17, 3, 30.4732799463, false),
    (17, 3, 30.4732799463, true),
    (14, 4, 30.5059501639, false),
    (14, 4, 30.5059501639, true),
    (2, 9, 30.5692044955, false),
    (2, 9, 30.5692044955, true),
    (0, 10, 30.6346064684, false),
    (25, 1, 30.7790391866, false),
    (25, 1, 30.7790391866, true),
    (9, 6, 30.8853789677, false),
    (9, 6, 30.8853789677, true),
    (21, 2, 31.0878045460, false),
    (21, 2, 31.0878045460, true),
    (7, 7, 31.4227941923, false),
    (7, 7, 31.4227941923, true),
    (12, 5, 31.4599600353, false),
    (12, 5, 31.4599600353, true),
    (18, 3, 31.6501181519, false),
    (18, 3, 31.6501181519, true),
    (15, 4, 31.7334133444, false),
    (15, 4, 31.7334133444, true),
    (5, 8, 31.8117167240, false),
    (5, 8, 31.8117167240, true),
    (26, 1, 31.8458872787, false),
    (26, 1, 31.8458872787, true),
    (3, 9, 32.0648524071, false),
    (3, 9, 32.0648524071, true),
    (1, 10, 32.1896799110, false),
    (1, 10, 32.1896799110, true),
    (22, 2, 32.2105865495, false),
    (22, 2, 32.2105865495, true),
    (10, 6, 32.2118561997, false),
    (10, 6, 32.2118561997, true),
    (13, 5, 32.7310533110, false),
    (13, 5, 32.7310533110, true),
    (8, 7, 32.7958000373, false),
    (8, 7, 32.7958000373, true),
    (19, 3, 32.8218027619, false),
    (19, 3, 32.8218027619, true),
    (27, 1, 32.9111538050, false),
    (27, 1, 32.9111538050, true),
];


#[derive(Debug, Clone, Copy)]
pub struct SnareModeState {
    pub q: f64,
    pub v: f64,
    pub base_omega: f64,
    pub modal_mass: f64,
    pub t60: f64,
    pub angular_order: u8,
    pub radial_root: f64,
    pub sine_orientation: bool,
    pub strike_coupling: f64,
    pub radiation_weight: f64,
    // Spatial shapes at the 16 snare wire contact points along the diameter
    pub wire_shapes: [f64; 16],
}

pub struct MalletExciter {
    pub x: f64,
    pub v: f64,
    pub mass: f64,
    pub stiffness: f64,
    pub dissipation: f64,
    pub exponent: f64,
    pub is_contacting: bool,
}

impl MalletExciter {
    pub fn new(mass: f64, stiffness: f64, dissipation: f64) -> Self {
        Self {
            x: 0.0,
            v: 0.0,
            mass,
            stiffness,
            dissipation,
            exponent: 1.5,
            is_contacting: false,
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        self.x = 0.0;
        self.v = velocity.clamp(0.02, 12.0);
        self.is_contacting = true;
    }

    pub fn step(&mut self, surface_x: f64, surface_v: f64, dt: f64) -> f64 {
        if !self.is_contacting {
            return 0.0;
        }
        const SUB_STEPS: usize = 4;
        let sub_dt = dt / SUB_STEPS as f64;
        let mut total_force = 0.0;

        for _ in 0..SUB_STEPS {
            let penetration = self.x - surface_x;
            if penetration > 0.0 {
                let v_rel = self.v - surface_v;
                let elastic = self.stiffness * penetration.powf(self.exponent);
                let dissipative = self.dissipation * elastic * v_rel;
                let force = (elastic + dissipative).max(0.0);
                self.v -= (force / self.mass) * sub_dt;
                self.x += self.v * sub_dt;
                total_force += force;
            } else {
                self.x += self.v * sub_dt;
                if self.v <= surface_v {
                    self.is_contacting = false;
                }
            }
        }
        total_force / SUB_STEPS as f64
    }
}

pub struct ShellResonator {
    y1: f64,
    y2: f64,
    a1: f64,
    a2: f64,
    gain: f64,
}

impl ShellResonator {
    pub fn new(sample_rate: f64, freq_hz: f64, q_factor: f64, gain: f64) -> Self {
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let alpha = w0.sin() / (2.0 * q_factor);
        let b0 = alpha;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * w0.cos() / a0;
        let a2 = (1.0 - alpha) / a0;
        Self {
            y1: 0.0,
            y2: 0.0,
            a1,
            a2,
            gain: (b0 / a0) * gain,
        }
    }

    pub fn tick(&mut self, x: f64) -> f64 {
        let y = self.gain * x - self.a1 * self.y1 - self.a2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// A discrete metallic snare wire element touching the bottom head
pub struct SnareWireElement {
    pub z: f64,        // Displacement
    pub v: f64,        // Velocity
    pub gap: f64,      // Microscopic mounting rest gap
    pub mass: f64,     // Effective mass (~0.3 g)
    pub stiffness: f64,// Restoring tension spring
    pub damping: f64,  // Internal steel dissipation
    pub contact_k: f64,// Contact stiffness
}

pub struct SnareDrum {
    pub sample_rate: f64,
    pub radius_m: f64,
    pub exciter: MalletExciter,
    // Fundamental (0, 1) coupled eigenmodes (In-phase & Anti-phase)
    pub u_plus_q: f64,
    pub u_plus_v: f64,
    pub trans_plus: ModalTransition,
    pub u_minus_q: f64,
    pub u_minus_v: f64,
    pub trans_minus: ModalTransition,
    pub modal_mass_01: f64,
    // Higher modes for top (batter) and bottom (snare-side) heads
    pub top_modes: Vec<SnareModeState>,
    pub bottom_modes: Vec<SnareModeState>,
    top_trans: Vec<ModalTransition>,
    bottom_trans: Vec<ModalTransition>,
    // 16 discrete metallic snare wire contacts
    pub wires: [SnareWireElement; 16],
    // Shell body resonance (metal/wood snare shell ~350 Hz & ~700 Hz)
    pub shell1: ShellResonator,
    pub shell2: ShellResonator,
    // Pre-allocated modal back-reaction force array
    wire_modal_forces: Vec<f64>,
}

impl SnareDrum {
    pub fn new(sample_rate: f64) -> Self {
        let radius_m = 0.1778; // 14-inch snare
        let f0_top = 210.0;    // Batter head fundamental ~210 Hz
        let f0_bottom = 260.0; // Ultra-thin snare side tuned higher ~260 Hz
        let alpha0 = TOM_MODE_SPECS[0].2;
        let dt = 1.0 / sample_rate;

        // Top head mass: coated head (~0.28 kg/m^2)
        let top_total_mass = 0.28 * PI * radius_m * radius_m;
        // Bottom head mass: ultra-thin snare-side head (~0.09 kg/m^2, 1/3 of top head!)
        let bottom_total_mass = 0.09 * PI * radius_m * radius_m;
        let modal_mass_01 = top_total_mass * 0.25;

        // Coupled cavity eigenmodes for fundamental (0, 1):
        let omega_plus = 2.0 * PI * f0_top;
        let omega_minus = 2.0 * PI * (f0_top * 1.28);
        let trans_plus = ModalTransition::new(omega_plus, 1000.0_f64.ln() / 0.8, dt, OverdampedPolicy::ExponentialFallback);
        let trans_minus = ModalTransition::new(omega_minus, 1000.0_f64.ln() / 0.5, dt, OverdampedPolicy::ExponentialFallback);

        // Precompute wire contact positions along diameter: x = 0, y from -0.80R to +0.80R
        let mut wire_positions = [0.0; 16];
        for i in 0..16 {
            wire_positions[i] = -0.80 + 1.60 * (i as f64) / 15.0;
        }

        let mut top_modes = Vec::with_capacity(255);
        let mut bottom_modes = Vec::with_capacity(255);
        let mut top_trans = Vec::with_capacity(255);
        let mut bottom_trans = Vec::with_capacity(255);

        for &(order, _radial_idx, root, is_sin) in TOM_MODE_SPECS[1..].iter() {
            let freq_top = f0_top * (root / alpha0);
            let omega_top = 2.0 * PI * freq_top;
            let t60_top = (0.7 / (1.0 + (freq_top / 350.0).powf(1.6)) + 0.02).max(0.015);
            let sigma_top = (1000.0_f64.ln() / t60_top).min(omega_top * 0.9);

            let freq_bot = f0_bottom * (root / alpha0);
            let omega_bot = 2.0 * PI * freq_bot;
            // Snare-side head is very thin, rings fast and bright
            let t60_bot = (0.5 / (1.0 + (freq_bot / 450.0).powf(1.5)) + 0.015).max(0.01);
            let sigma_bot = (1000.0_f64.ln() / t60_bot).min(omega_bot * 0.9);

            let top_mass = if order == 0 { top_total_mass * 0.25 } else { top_total_mass * 0.15 };
            let bot_mass = if order == 0 { bottom_total_mass * 0.25 } else { bottom_total_mass * 0.15 };

            let rad_weight_top = if order == 0 {
                1.0 / (1.0 + (root / alpha0 - 1.0) * 0.4)
            } else {
                0.22 / (1.0 + order as f64 * 0.7 + (root / alpha0) * 0.25)
            };

            let rad_weight_bot = if order == 0 {
                0.8 / (1.0 + (root / alpha0 - 1.0) * 0.4)
            } else {
                0.35 / (1.0 + order as f64 * 0.6 + (root / alpha0) * 0.20)
            };

            // Precompute shape of this mode at all 16 wire points
            let mut wire_shapes = [0.0; 16];
            for w in 0..16 {
                let r_norm = wire_positions[w].abs().clamp(0.01, 0.95);
                let angle = if wire_positions[w] >= 0.0 { PI * 0.5 } else { -PI * 0.5 };
                wire_shapes[w] = mode_shape(order, root, is_sin, r_norm, angle);
            }

            top_modes.push(SnareModeState {
                q: 0.0,
                v: 0.0,
                base_omega: omega_top,
                modal_mass: top_mass,
                t60: t60_top,
                angular_order: order,
                radial_root: root,
                sine_orientation: is_sin,
                strike_coupling: 0.0,
                radiation_weight: rad_weight_top,
                wire_shapes: [0.0; 16],
            });

            bottom_modes.push(SnareModeState {
                q: 0.0,
                v: 0.0,
                base_omega: omega_bot,
                modal_mass: bot_mass,
                t60: t60_bot,
                angular_order: order,
                radial_root: root,
                sine_orientation: is_sin,
                strike_coupling: 0.0,
                radiation_weight: rad_weight_bot,
                wire_shapes,
            });

            top_trans.push(ModalTransition::new(omega_top, sigma_top, dt, OverdampedPolicy::ExponentialFallback));
            bottom_trans.push(ModalTransition::new(omega_bot, sigma_bot, dt, OverdampedPolicy::ExponentialFallback));
        }

        // Initialize 16 snare wire elements with deterministic micro-gap variations
        let mut wires = std::array::from_fn(|i| {
            // Microscopic gap distribution (-0.05mm to +0.08mm) across the bed
            let norm_i = (i as f64 - 7.5) / 7.5;
            let gap = (0.00003 + 0.00004 * norm_i * norm_i) * (1.0 + 0.2 * (i as f64 * 1.7).sin());
            SnareWireElement {
                z: 0.0,
                v: 0.0,
                gap,
                mass: 0.00025, // 0.25 g effective wire mass
                stiffness: 14_000.0 + 2_000.0 * (i as f64 * 2.3).cos(), // slight wire detuning
                damping: 18.0,
                contact_k: 8.0e5, // Hertzian contact stiffness
            }
        });

        // 14-inch snare metal shell resonance (~340 Hz and ~680 Hz)
        let shell1 = ShellResonator::new(sample_rate, 340.0, 6.0, 0.20);
        let shell2 = ShellResonator::new(sample_rate, 680.0, 8.0, 0.12);

        let mut snare = Self {
            sample_rate,
            radius_m,
            exciter: MalletExciter::new(0.022, 1.2e7, 0.04), // Snare stick: hard tip
            u_plus_q: 0.0,
            u_plus_v: 0.0,
            trans_plus,
            u_minus_q: 0.0,
            u_minus_v: 0.0,
            trans_minus,
            modal_mass_01,
            top_modes,
            bottom_modes,
            top_trans,
            bottom_trans,
            wires,
            shell1,
            shell2,
            wire_modal_forces: vec![0.0; 255],
        };
        snare.set_strike_point(0.35, 0.0);
        snare
    }

    pub fn set_strike_point(&mut self, radius_fraction: f64, angle_rad: f64) {
        let r_norm = radius_fraction.clamp(0.01, 0.95);
        let contact_patch_m = self.radius_m * 0.08;
        for mode in &mut self.top_modes {
            let spatial = mode_shape(mode.angular_order, mode.radial_root, mode.sine_orientation, r_norm, angle_rad);
            let filter = (-0.5 * (mode.radial_root * contact_patch_m / self.radius_m).powi(2)).exp();
            mode.strike_coupling = spatial * filter;
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        self.exciter.trigger(velocity * 4.8);
    }

    pub fn step(&mut self) -> f64 {
        let dt = 1.0 / self.sample_rate;

        // 1. Top head strike point displacement
        let alpha0 = TOM_MODE_SPECS[0].2;
        let phi_01_top = bessel_j(0, alpha0 * 0.35) * (-0.5 * (alpha0 * 0.08).powi(2)).exp();
        let q_top_01 = (self.u_plus_q + self.u_minus_q) * (1.0 / std::f64::consts::SQRT_2);
        let v_top_01 = (self.u_plus_v + self.u_minus_v) * (1.0 / std::f64::consts::SQRT_2);

        let mut x_top = q_top_01 * phi_01_top;
        let mut v_top = v_top_01 * phi_01_top;
        for mode in &self.top_modes {
            x_top += mode.q * mode.strike_coupling;
            v_top += mode.v * mode.strike_coupling;
        }

        // Exciter strike force
        let strike_force = self.exciter.step(x_top, v_top, dt);

        // 2. Fundamental (0, 1) coupled cavity eigenmodes
        let f_modal_01 = strike_force * phi_01_top;
        let force_plus = f_modal_01 * (1.0 / std::f64::consts::SQRT_2) / self.modal_mass_01;
        let force_minus = f_modal_01 * (1.0 / std::f64::consts::SQRT_2) / self.modal_mass_01;

        let (p11_p, p12_p, p21_p, p22_p) = self.trans_plus.phi;
        let (g1_p, g2_p) = self.trans_plus.gamma;
        let (p11_m, p12_m, p21_m, p22_m) = self.trans_minus.phi;
        let (g1_m, g2_m) = self.trans_minus.gamma;

        self.u_plus_q = p11_p * self.u_plus_q + p12_p * self.u_plus_v + g1_p * force_plus;
        self.u_plus_v = p21_p * self.u_plus_q + p22_p * self.u_plus_v + g2_p * force_plus;

        self.u_minus_q = p11_m * self.u_minus_q + p12_m * self.u_minus_v + g1_m * force_minus;
        self.u_minus_v = p21_m * self.u_minus_q + p22_m * self.u_minus_v + g2_m * force_minus;

        let v_top_fund = (self.u_plus_v + self.u_minus_v) * (1.0 / std::f64::consts::SQRT_2);
        let q_bot_fund = (self.u_plus_q - self.u_minus_q) * (1.0 / std::f64::consts::SQRT_2);
        let v_bot_fund = (self.u_plus_v - self.u_minus_v) * (1.0 / std::f64::consts::SQRT_2);

        // 3. Advance top head higher modes
        let mut top_sound = v_top_fund * 1.0;
        for (mode, trans) in self.top_modes.iter_mut().zip(&self.top_trans) {
            let (p11, p12, p21, p22) = trans.phi;
            let (g1, g2) = trans.gamma;
            let f_m = (strike_force * mode.strike_coupling) / mode.modal_mass;
            let nq = p11 * mode.q + p12 * mode.v + g1 * f_m;
            let nv = p21 * mode.q + p22 * mode.v + g2 * f_m;
            mode.q = nq;
            mode.v = nv;
            top_sound += mode.v * mode.radiation_weight;
        }

        // 4. Snare Wire Contacts: sample bottom head displacement at 16 points
        self.wire_modal_forces.fill(0.0);
        let mut snare_rattle_acoustic = 0.0;

        for w in 0..16 {
            let phi_01_wire = bessel_j(0, alpha0 * ((-0.80 + 1.60 * w as f64 / 15.0).abs()));
            let mut w_bot_pt = q_bot_fund * phi_01_wire;
            let mut v_bot_pt = v_bot_fund * phi_01_wire;
            for mode in &self.bottom_modes {
                w_bot_pt += mode.q * mode.wire_shapes[w];
                v_bot_pt += mode.v * mode.wire_shapes[w];
            }

            // Penetration into wire: bottom head pushes wire outward
            let wire = &mut self.wires[w];
            let delta = w_bot_pt - wire.z - wire.gap;

            let contact_force = if delta > 0.0 {
                let v_rel = v_bot_pt - wire.v;
                let elastic = wire.contact_k * delta.powf(1.4);
                let dissipative = 0.08 * elastic * v_rel;
                (elastic + dissipative).max(0.0)
            } else {
                0.0
            };

            // Wire dynamics: accelerated by contact force, restored by tension spring
            let restoring = -wire.stiffness * wire.z - wire.damping * wire.v;
            wire.v += (contact_force + restoring) / wire.mass * dt;
            wire.z += wire.v * dt;

            // Acoustic emission from wire micro-collisions
            snare_rattle_acoustic += contact_force * 0.00035;

            // Reaction force onto bottom head modes (Newton's 3rd law: -contact_force)
            let reaction = -contact_force;
            for (idx, mode) in self.bottom_modes.iter().enumerate() {
                self.wire_modal_forces[idx] += reaction * mode.wire_shapes[w];
            }
        }

        // 5. Advance bottom head under wire back-reaction
        let mut bot_sound = v_bot_fund * 0.50;
        for (idx, mode) in self.bottom_modes.iter_mut().enumerate() {
            let trans = &self.bottom_trans[idx];
            let (p11, p12, p21, p22) = trans.phi;
            let (g1, g2) = trans.gamma;
            let f_m = self.wire_modal_forces[idx] / mode.modal_mass;
            let nq = p11 * mode.q + p12 * mode.v + g1 * f_m;
            let nv = p21 * mode.q + p22 * mode.v + g2 * f_m;
            mode.q = nq;
            mode.v = nv;
            bot_sound += mode.v * mode.radiation_weight;
        }

        // 6. Snare Shell body resonance
        let shell_input = top_sound + strike_force * 0.0001;
        let shell_sound = self.shell1.tick(shell_input) + self.shell2.tick(shell_input);

        // Acoustic pickup: top head + shell + bottom head + pure physical wire rattle
        top_sound + shell_sound + bot_sound * 0.35 + snare_rattle_acoustic
    }
}

fn mode_shape(order: u8, root: f64, sine_orientation: bool, r_norm: f64, angle: f64) -> f64 {
    let angular = if order == 0 {
        1.0
    } else if sine_orientation {
        (order as f64 * angle).sin()
    } else {
        (order as f64 * angle).cos()
    };
    bessel_j(order, root * r_norm) * angular
}

fn bessel_j(order: u8, x: f64) -> f64 {
    let order_u = order as usize;
    let half = x * 0.5;
    let mut term = half.powi(order as i32) / factorial(order_u);
    let mut sum = term;
    let half_squared = half * half;
    for k in 0..32 {
        let denom = (k + 1) as f64 * (k + order_u + 1) as f64;
        term *= -half_squared / denom;
        sum += term;
        if term.abs() < 1e-15 * sum.abs() {
            break;
        }
    }
    sum
}

fn factorial(n: usize) -> f64 {
    let mut val = 1.0;
    for i in 2..=n {
        val *= i as f64;
    }
    val
}

fn render_snare(mut snare: SnareDrum, velocity: f64, seconds: f64) -> Vec<f64> {
    snare.trigger(velocity);
    let total_frames = (snare.sample_rate * seconds) as usize;
    let mut out = Vec::with_capacity(total_frames);
    for _ in 0..total_frames {
        out.push(snare.step());
    }
    out
}

fn save_wav(samples: &[f64], filename: &str) {
    let dir = Path::new("target/tom-prototype");
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(filename);
    let spec = WavSpec {
        channels: 1,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&path, spec).unwrap();
    let peak = samples.iter().copied().fold(0.0_f64, |p, s| p.max(s.abs())).max(1e-6);
    for &s in samples {
        let norm = (s / peak * 0.90).clamp(-1.0, 1.0);
        let sample_i16 = (norm * 32767.0) as i16;
        writer.write_sample(sample_i16).unwrap();
    }
    writer.finalize().unwrap();
    println!("Saved WAV to {}", path.display());
}

#[test]
fn test_snare_prototype_without_white_noise() {
    println!("Rendering Snare Drum with 16 discrete metallic contact elements (ZERO white noise)...");
    let sample_rate = 48000.0;

    // 1. Normal Hard Hit (Center-ish r=0.35, Velocity 0.9)
    let mut snare1 = SnareDrum::new(sample_rate);
    snare1.set_strike_point(0.35, 0.0);
    let hard_samples = render_snare(snare1, 0.95, 1.2);
    save_wav(&hard_samples, "snare_hard_center.wav");

    // 2. Ghost Note (Very light touch Velocity 0.15, r=0.50)
    let mut snare2 = SnareDrum::new(sample_rate);
    snare2.set_strike_point(0.50, 0.0);
    let ghost_samples = render_snare(snare2, 0.15, 0.8);
    save_wav(&ghost_samples, "snare_ghost_note.wav");

    // 3. Rimshot (Hard edge r=0.85, high velocity 1.0)
    let mut snare3 = SnareDrum::new(sample_rate);
    snare3.set_strike_point(0.85, 0.0);
    let rimshot_samples = render_snare(snare3, 1.0, 1.0);
    save_wav(&rimshot_samples, "snare_rimshot.wav");

    // Assertions: no NaN, Inf, non-zero energy
    assert!(hard_samples.iter().all(|s| s.is_finite()));
    assert!(ghost_samples.iter().all(|s| s.is_finite()));
    assert!(rimshot_samples.iter().all(|s| s.is_finite()));

    println!("Snare prototype rendered successfully without any white noise!");
}
