//! Fretboard topology, 6-string voicing allocator, and MPE routing.

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
    /// Open string MIDI notes for strings 1..=6
    pub open_notes: [u8; 6],
    /// Current virtual hand position on the neck (fret 0 to 24)
    pub hand_position: u8,
}

impl Default for FretboardRouter {
    fn default() -> Self {
        Self {
            open_notes: [64, 59, 55, 50, 45, 40], // E4, B3, G3, D3, A2, E2
            hand_position: 0, // open/first position
        }
    }
}

impl FretboardRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves which string (1..=6) and fret (0..=24) should play `midi_note`,
    /// considering which strings are currently held.
    pub fn allocate_note(
        &self,
        midi_note: u8,
        strings_held: &[bool; 6],
    ) -> Option<FretboardLocation> {
        let mut candidates = Vec::new();

        for i in 0..6 {
            let open = self.open_notes[i];
            if midi_note >= open {
                let fret = midi_note - open;
                if fret <= 24 {
                    // String index is 1-based (1..=6)
                    let string_idx = (i + 1) as u8;
                    let is_held = strings_held[i];

                    // Cost function:
                    // - Penalty if string is already held
                    // - Distance from current hand position
                    let mut cost = (fret as i32 - self.hand_position as i32).abs();
                    if is_held {
                        cost += 100;
                    }
                    candidates.push((cost, string_idx, fret));
                }
            }
        }

        if candidates.is_empty() {
            None
        } else {
            candidates.sort_by_key(|&(cost, _, _)| cost);
            Some(FretboardLocation {
                string_index: candidates[0].1,
                fret: candidates[0].2,
            })
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
