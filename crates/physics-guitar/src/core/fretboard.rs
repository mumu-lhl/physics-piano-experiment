//! Fretboard topology, ergonomic hand position optimization, and MPE routing.
//!
//! Implements docx Chapter 6:
//! Cost-minimizing ergonomic fret hand position allocation:
//! C = w_pos * D_pos + w_span * D_span + w_cross * D_cross + w_open * P_open

/// Result of resolving a note on the fretboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FretboardLocation {
    /// String index: 1 (high E) to 6 (low E)
    pub string_index: u8,
    /// Fret number: 0 (open) to 24
    pub fret: u8,
}

#[derive(Debug, Clone)]
pub struct FretboardRouter {
    /// Open string MIDI notes for strings 1..=6 (E4, B3, G3, D3, A2, E2)
    pub open_notes: [u8; 6],
    /// Current virtual left-hand position on the neck (fret 0 to 24)
    pub hand_position: u8,
    /// Weights for biomechanical cost optimization
    pub weight_pos: f64,
    pub weight_span: f64,
    pub weight_open: f64,
}

impl Default for FretboardRouter {
    fn default() -> Self {
        Self {
            open_notes: [64, 59, 55, 50, 45, 40], // String 1 (E4) -> String 6 (E2)
            hand_position: 2, // Default 1st/2nd position
            weight_pos: 1.0,
            weight_span: 3.5,
            weight_open: 2.0,
        }
    }
}

impl FretboardRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the current hand position explicitly (e.g. for barre chords or high solos).
    pub fn set_hand_position(&mut self, pos: u8) {
        self.hand_position = pos.clamp(0, 24);
    }

    /// Optimal hand position routing (docx Chapter 6).
    /// Evaluates all possible (string, fret) candidates that produce `midi_note`,
    /// computing the total ergonomic cost:
    /// - D_pos: Distance of fret from hand position
    /// - D_span: Stretch beyond 4 frets relative to currently sounding frets
    /// - D_cross: Hard collision penalty if string is currently vibrating
    /// - P_open: Open string preference / suppression based on hand position
    pub fn allocate_note(
        &mut self,
        midi_note: u8,
        strings_held: &[bool; 6],
        active_frets: &[Option<u8>; 6],
    ) -> Option<FretboardLocation> {
        let mut candidates = Vec::new();

        for s_idx in 0..6 {
            let open = self.open_notes[s_idx];
            if midi_note >= open {
                let fret = midi_note - open;
                if fret <= 24 {
                    let is_held = strings_held[s_idx];
                    let string_num = (s_idx + 1) as u8;

                    // 1. Hand position movement cost D_pos
                    let d_pos = if fret == 0 {
                        0.0
                    } else {
                        let diff = fret as f64 - self.hand_position as f64;
                        diff.powi(2)
                    };

                    // 2. Finger span stretch cost D_span
                    let mut d_span = 0.0;
                    if fret > 0 {
                        for other_fret in active_frets.iter().flatten() {
                            if *other_fret > 0 {
                                let span = (fret as i32 - *other_fret as i32).abs();
                                if span > 4 {
                                    // Super-linear stretch penalty when hand span exceeds 4 frets
                                    d_span += ((span - 4) as f64).powi(2) * 12.0;
                                }
                            }
                        }
                    }

                    // 3. String collision penalty D_cross
                    let d_cross = if is_held { 1000.0 } else { 0.0 };

                    // 4. Open string fitness P_open
                    // If playing high up the neck (hand_pos >= 7), suppress accidental open strings;
                    // If in low positions (hand_pos <= 3), open strings have neutral/rewarded cost.
                    let p_open = if fret == 0 {
                        if self.hand_position >= 7 { 45.0 } else { -5.0 }
                    } else {
                        0.0
                    };

                    let total_cost = self.weight_pos * d_pos
                        + self.weight_span * d_span
                        + d_cross
                        + self.weight_open * p_open;

                    candidates.push((total_cost, string_num, fret));
                }
            }
        }

        if candidates.is_empty() {
            None
        } else {
            // Sort by lowest cost
            candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let chosen_str = candidates[0].1;
            let chosen_fret = candidates[0].2;

            // Dynamically update hand position if non-open fret was chosen
            if chosen_fret > 0 {
                // Smooth hand inertia update (moves towards new active fret cluster)
                let new_pos = (self.hand_position as f64 * 0.7 + chosen_fret as f64 * 0.3).round() as u8;
                self.hand_position = new_pos.clamp(0, 24);
            }

            Some(FretboardLocation {
                string_index: chosen_str,
                fret: chosen_fret,
            })
        }
    }

    /// Fast standard routing for single-note lines with fallback.
    pub fn route_note(&mut self, midi_note: u8) -> (usize, u8) {
        let dummy_held = [false; 6];
        let dummy_frets = [None; 6];
        if let Some(loc) = self.allocate_note(midi_note, &dummy_held, &dummy_frets) {
            ((loc.string_index - 1) as usize, loc.fret)
        } else {
            (0, 0)
        }
    }

    /// MPE allocation: Maps MPE channel (Channel 2..=7) directly to strings 1..=6.
    pub fn allocate_mpe_note(&self, channel: u8, midi_note: u8) -> Option<FretboardLocation> {
        if (2..=7).contains(&channel) {
            let string_idx = channel - 1; // Channel 2 -> String 1, Channel 7 -> String 6
            let open = self.open_notes[(string_idx - 1) as usize];
            if midi_note >= open {
                let fret = midi_note - open;
                if fret <= 24 {
                    return Some(FretboardLocation {
                        string_index: string_idx,
                        fret,
                    });
                }
            }
        }
        None
    }
}
