//! Left-hand finger squeak and wound-string slide friction acoustics.
//!
//! Models the physical tactile friction when fingers move across the wire
//! wrapping coils of strings 4, 5, and 6 (D, A, low E) during position shifts
//! and legato slides (docx Chapter 2 Coulomb-Stribeck friction & Chapter 6 hand shifting).

use crate::core::body::BiquadFilter;

/// Generator for wound string finger squeak / slide acoustic noise.
#[derive(Debug, Clone)]
pub struct FingerSqueakGenerator {
    pub sample_rate: f64,
    /// Master squeak volume [0.0 = disabled, 1.0 = standard authentic friction]
    pub squeak_level: f64,
    /// Current friction velocity amplitude envelope
    pub envelope: f64,
    /// Envelope decay rate per sample
    decay_rate: f64,
    /// Center frequency of current squeak burst (Hz)
    pub current_fc: f64,
    /// Resonant bandpass filter simulating winding resonance
    pub filter: BiquadFilter,
    /// Last hand position on neck (0..24)
    pub last_hand_pos: u8,
    /// Pseudo-random number generator state (xorshift64)
    rng_state: u64,
}

impl FingerSqueakGenerator {
    pub fn new(sample_rate: f64) -> Self {
        // Bandpass filter centered at 3100 Hz with Q=2.2 (typical acoustic wound string squeak resonance)
        let filter = BiquadFilter::new_bandpass(3100.0, 2.2, sample_rate);
        Self {
            sample_rate,
            squeak_level: 0.40, // 40% natural studio squeak by default
            envelope: 0.0,
            decay_rate: (-1.0 / (0.09 * sample_rate)).exp(), // ~90ms decay
            current_fc: 3100.0,
            filter,
            last_hand_pos: 2,
            rng_state: 0x8543_9281_4472_9103,
        }
    }

    /// Fast, deterministic zero-allocation PRNG for tactile friction grain.
    #[inline(always)]
    fn next_noise(&mut self) -> f64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        // Map to [-1.0, 1.0]
        (self.rng_state as f64 / u64::MAX as f64) * 2.0 - 1.0
    }

    /// Triggers a finger squeak pulse when hand position shifts across frets.
    /// `delta_frets` is the jump distance (e.g. 2 to 7 = 5 frets).
    /// `string_index` is 0..5 (0 = High E, 5 = Low E). Squeaks only occur on wound strings (indices 3, 4, 5).
    pub fn trigger_shift(&mut self, string_index: usize, delta_frets: u8, speed_mult: f64) {
        if self.squeak_level <= 0.0 || delta_frets < 2 || string_index < 3 {
            return;
        }

        // Slide velocity estimation: distance / duration
        let shift_dist_meters = (delta_frets as f64) * 0.025; // ~25mm per fret
        let slide_speed = (shift_dist_meters * 12.0 * speed_mult).clamp(0.4, 2.5); // m/s

        // Winding coil pitch d_wrap ~= 0.35 mm
        let d_wrap = 0.00035;
        let fc = (slide_speed / d_wrap).clamp(1800.0, 4800.0);
        self.current_fc = fc;

        // Reconfigure bandpass filter for the dynamic pitch of this slide
        self.filter = BiquadFilter::new_bandpass(fc, 2.5, self.sample_rate);

        // Amplitude proportional to distance and string diameter (string 5/6 low E/A have deepest ribs)
        let string_rib_weight = match string_index {
            5 => 1.0,  // Low E (thickest round-wound wire)
            4 => 0.85, // A string
            3 => 0.65, // D string
            _ => 0.0,  // Plain unwound strings G, B, high E have smooth surfaces (no squeak)
        };

        let initial_amp = ((delta_frets as f64 * 0.15).min(1.0) * string_rib_weight * self.squeak_level).min(0.85);
        self.envelope = initial_amp;

        // Decay duration between 50ms (short jump) and 130ms (long slide across neck)
        let decay_time_sec = (0.05 + 0.015 * delta_frets as f64).clamp(0.04, 0.14);
        self.decay_rate = (-1.0 / (decay_time_sec * self.sample_rate)).exp();
    }

    /// Evaluates one audio sample of finger squeak friction noise.
    #[inline(always)]
    pub fn process_sample(&mut self) -> f64 {
        if self.envelope < 1e-5 {
            self.envelope = 0.0;
            return 0.0;
        }

        // Generate friction noise grain
        let raw_grain = self.next_noise();

        // Resonant filtering through winding rib acoustics
        let filtered = self.filter.process(raw_grain);

        // Modulate with envelope
        let output = filtered * self.envelope * 0.45;

        // Step exponential decay
        self.envelope *= self.decay_rate;

        output
    }
}
