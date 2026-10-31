//! Chord library for standard tuning (EADGBE).
//!
//! A shape is stored low string → high string (string 6 → string 1),
//! matching conventional chord diagrams. `None` means the string is muted
//! ("x" in a diagram). The engine numbers strings the other way round
//! (index 0 = high E); [`ChordShape::engine_notes`] performs that conversion.
//!
//! The library covers every root × quality combination (12 × 5 = 60 shapes):
//! roots with a classic open fingering use it, everything else uses a movable
//! barre shape — E-shape (root on the low E string) or A-shape (root on the
//! A string), whichever puts the barre closer to the nut. The pitch-class
//! test below verifies all 60 shapes mechanically, so a wrong fingering
//! cannot slip in silently.

/// Open-string MIDI notes, low string → high string (E2 A2 D3 G3 B3 E4).
const OPEN_NOTES: [u8; 6] = [40, 45, 50, 55, 59, 64];

/// Chord root (pitch class).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChordRoot {
    C,
    Cs,
    D,
    Ds,
    E,
    F,
    Fs,
    G,
    Gs,
    A,
    As,
    B,
}

impl ChordRoot {
    pub const ALL: [ChordRoot; 12] = [
        ChordRoot::C,
        ChordRoot::Cs,
        ChordRoot::D,
        ChordRoot::Ds,
        ChordRoot::E,
        ChordRoot::F,
        ChordRoot::Fs,
        ChordRoot::G,
        ChordRoot::Gs,
        ChordRoot::A,
        ChordRoot::As,
        ChordRoot::B,
    ];

    pub fn pitch_class(self) -> u8 {
        match self {
            ChordRoot::C => 0,
            ChordRoot::Cs => 1,
            ChordRoot::D => 2,
            ChordRoot::Ds => 3,
            ChordRoot::E => 4,
            ChordRoot::F => 5,
            ChordRoot::Fs => 6,
            ChordRoot::G => 7,
            ChordRoot::Gs => 8,
            ChordRoot::A => 9,
            ChordRoot::As => 10,
            ChordRoot::B => 11,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ChordRoot::C => "C",
            ChordRoot::Cs => "C#",
            ChordRoot::D => "D",
            ChordRoot::Ds => "D#",
            ChordRoot::E => "E",
            ChordRoot::F => "F",
            ChordRoot::Fs => "F#",
            ChordRoot::G => "G",
            ChordRoot::Gs => "G#",
            ChordRoot::A => "A",
            ChordRoot::As => "A#",
            ChordRoot::B => "B",
        }
    }

    pub fn from_index(index: i32) -> ChordRoot {
        ChordRoot::ALL[(index.rem_euclid(12)) as usize]
    }
}

/// Chord quality (intervals in semitones above the root).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChordQuality {
    Major,
    Minor,
    Dominant7,
    Major7,
    Minor7,
}

impl ChordQuality {
    pub const ALL: [ChordQuality; 5] = [
        ChordQuality::Major,
        ChordQuality::Minor,
        ChordQuality::Dominant7,
        ChordQuality::Major7,
        ChordQuality::Minor7,
    ];

    /// Chord tones as semitone offsets from the root.
    pub fn intervals(self) -> &'static [u8] {
        match self {
            ChordQuality::Major => &[0, 4, 7],
            ChordQuality::Minor => &[0, 3, 7],
            ChordQuality::Dominant7 => &[0, 4, 7, 10],
            ChordQuality::Major7 => &[0, 4, 7, 11],
            ChordQuality::Minor7 => &[0, 3, 7, 10],
        }
    }

    /// Display suffix, e.g. "m", "7", "maj7", "m7" (major has none).
    pub fn suffix(self) -> &'static str {
        match self {
            ChordQuality::Major => "",
            ChordQuality::Minor => "m",
            ChordQuality::Dominant7 => "7",
            ChordQuality::Major7 => "maj7",
            ChordQuality::Minor7 => "m7",
        }
    }

    pub fn from_index(index: i32) -> ChordQuality {
        ChordQuality::ALL[(index.rem_euclid(5)) as usize]
    }
}

/// A fingering: frets low string → high string (string 6 → string 1).
/// `None` = muted string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChordShape {
    pub frets: [Option<u8>; 6],
}

impl ChordShape {
    /// Display name, e.g. "F#m7".
    pub fn name(root: ChordRoot, quality: ChordQuality) -> String {
        format!("{}{}", root.name(), quality.suffix())
    }

    /// Sounding pitch classes (0 = C .. 11 = B), low string → high string,
    /// muted strings skipped.
    pub fn sounding_pitch_classes(&self) -> Vec<u8> {
        self.frets
            .iter()
            .zip(OPEN_NOTES.iter())
            .filter_map(|(fret, open)| fret.map(|f| (open + f) % 12))
            .collect()
    }

    /// Sounding strings as (engine string index, fret) pairs, in ascending
    /// engine order (index 0 = high E first). The engine numbers strings
    /// high → low, the reverse of the stored low → high order.
    pub fn engine_notes(&self) -> Vec<(usize, u8)> {
        self.frets
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(low_to_high_idx, fret)| fret.map(|f| (5 - low_to_high_idx, f)))
            .collect()
    }

    /// Diagram text, low string → high string: "x 3 2 0 1 0" for C major.
    pub fn fingering_text(&self) -> String {
        self.frets
            .iter()
            .map(|fret| match fret {
                None => "x".to_string(),
                Some(f) => f.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

type Frets = [Option<u8>; 6];

/// Classic open fingerings, low → high. Only roots/qualities with a standard
/// open shape are listed; everything else falls through to barre shapes.
fn open_shape(root: ChordRoot, quality: ChordQuality) -> Option<ChordShape> {
    use ChordQuality::*;
    use ChordRoot::*;
    let frets: Frets = match (root, quality) {
        (C, Major) => [None, Some(3), Some(2), Some(0), Some(1), Some(0)],
        (C, Dominant7) => [None, Some(3), Some(2), Some(3), Some(1), Some(0)],
        (C, Major7) => [None, Some(3), Some(2), Some(0), Some(0), Some(0)],
        (G, Major) => [Some(3), Some(2), Some(0), Some(0), Some(0), Some(3)],
        (G, Dominant7) => [Some(3), Some(2), Some(0), Some(0), Some(0), Some(1)],
        (G, Major7) => [Some(3), Some(2), Some(0), Some(0), Some(0), Some(2)],
        (D, Major) => [None, None, Some(0), Some(2), Some(3), Some(2)],
        (D, Dominant7) => [None, None, Some(0), Some(2), Some(1), Some(2)],
        (D, Major7) => [None, None, Some(0), Some(2), Some(2), Some(2)],
        (D, Minor) => [None, None, Some(0), Some(2), Some(3), Some(1)],
        (D, Minor7) => [None, None, Some(0), Some(2), Some(1), Some(1)],
        (A, Major) => [None, Some(0), Some(2), Some(2), Some(2), Some(0)],
        (A, Dominant7) => [None, Some(0), Some(2), Some(0), Some(2), Some(0)],
        (A, Major7) => [None, Some(0), Some(2), Some(1), Some(2), Some(0)],
        (A, Minor) => [None, Some(0), Some(2), Some(2), Some(1), Some(0)],
        (A, Minor7) => [None, Some(0), Some(2), Some(0), Some(1), Some(0)],
        (E, Major) => [Some(0), Some(2), Some(2), Some(1), Some(0), Some(0)],
        (E, Dominant7) => [Some(0), Some(2), Some(0), Some(1), Some(0), Some(0)],
        (E, Major7) => [Some(0), Some(2), Some(1), Some(1), Some(0), Some(0)],
        (E, Minor) => [Some(0), Some(2), Some(2), Some(0), Some(0), Some(0)],
        (E, Minor7) => [Some(0), Some(2), Some(0), Some(0), Some(0), Some(0)],
        _ => return None,
    };
    Some(ChordShape { frets })
}

/// E-shape barre (root on the low E string), `f` = barre fret, low → high.
fn e_shape(f: u8, quality: ChordQuality) -> ChordShape {
    use ChordQuality::*;
    let frets: Frets = match quality {
        Major => [
            Some(f),
            Some(f + 2),
            Some(f + 2),
            Some(f + 1),
            Some(f),
            Some(f),
        ],
        Minor => [Some(f), Some(f + 2), Some(f + 2), Some(f), Some(f), Some(f)],
        Dominant7 => [Some(f), Some(f + 2), Some(f), Some(f + 1), Some(f), Some(f)],
        Major7 => [
            Some(f),
            Some(f + 2),
            Some(f + 1),
            Some(f + 1),
            Some(f),
            Some(f),
        ],
        Minor7 => [Some(f), Some(f + 2), Some(f), Some(f), Some(f), Some(f)],
    };
    ChordShape { frets }
}

/// A-shape barre (root on the A string), `f` = barre fret, low → high.
fn a_shape(f: u8, quality: ChordQuality) -> ChordShape {
    use ChordQuality::*;
    let frets: Frets = match quality {
        Major => [
            None,
            Some(f),
            Some(f + 2),
            Some(f + 2),
            Some(f + 2),
            Some(f),
        ],
        Minor => [
            None,
            Some(f),
            Some(f + 2),
            Some(f + 2),
            Some(f + 1),
            Some(f),
        ],
        Dominant7 => [None, Some(f), Some(f + 2), Some(f), Some(f + 2), Some(f)],
        Major7 => [
            None,
            Some(f),
            Some(f + 2),
            Some(f + 1),
            Some(f + 2),
            Some(f),
        ],
        Minor7 => [None, Some(f), Some(f + 2), Some(f), Some(f + 1), Some(f)],
    };
    ChordShape { frets }
}

/// The fingering for a chord: the classic open shape when one exists,
/// otherwise the movable barre shape whose barre sits closest to the nut
/// (E-shape and A-shape frets can never tie: they differ by 5 semitones).
pub fn chord_shape(root: ChordRoot, quality: ChordQuality) -> ChordShape {
    if let Some(shape) = open_shape(root, quality) {
        return shape;
    }
    let pc = root.pitch_class();
    // Distance in semitones from the E string (E = 4) and A string (A = 9).
    let fe = (pc + 12 - 4) % 12;
    let fa = (pc + 12 - 9) % 12;
    if fa < fe {
        a_shape(fa, quality)
    } else {
        e_shape(fe, quality)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every shape must sound only chord tones, must include the root and
    /// the third, and (for seventh chords) the seventh. The fifth may be
    /// omitted — standard practice (e.g. C7 x32310 has no fifth).
    #[test]
    fn all_shapes_sound_valid_chord_tones() {
        for root in ChordRoot::ALL {
            for quality in ChordQuality::ALL {
                let shape = chord_shape(root, quality);
                let tones: HashSet<u8> = quality
                    .intervals()
                    .iter()
                    .map(|i| (root.pitch_class() + i) % 12)
                    .collect();
                let sounding = shape.sounding_pitch_classes();
                assert!(
                    !sounding.is_empty(),
                    "{}{} sounds nothing",
                    root.name(),
                    quality.suffix()
                );
                for pc in &sounding {
                    assert!(
                        tones.contains(pc),
                        "{}{} sounds non-chord tone {} in {:?}",
                        root.name(),
                        quality.suffix(),
                        pc,
                        shape.frets
                    );
                }
                let sounding_set: HashSet<u8> = sounding.into_iter().collect();
                assert!(
                    sounding_set.contains(&root.pitch_class()),
                    "{}{} misses its root in {:?}",
                    root.name(),
                    quality.suffix(),
                    shape.frets
                );
                let third = (root.pitch_class() + quality.intervals()[1]) % 12;
                assert!(
                    sounding_set.contains(&third),
                    "{}{} misses its third in {:?}",
                    root.name(),
                    quality.suffix(),
                    shape.frets
                );
                if quality.intervals().len() == 4 {
                    let seventh = (root.pitch_class() + quality.intervals()[3]) % 12;
                    assert!(
                        sounding_set.contains(&seventh),
                        "{}{} misses its seventh in {:?}",
                        root.name(),
                        quality.suffix(),
                        shape.frets
                    );
                }
                for fret in shape.frets.iter().flatten() {
                    assert!(
                        *fret <= 24,
                        "{}{} has out-of-range fret {}",
                        root.name(),
                        quality.suffix(),
                        fret
                    );
                }
            }
        }
    }

    #[test]
    fn open_shapes_match_standard_fingerings() {
        use ChordQuality::*;
        use ChordRoot::*;
        // Low → high, x = muted. These are the textbook fingerings.
        assert_eq!(
            chord_shape(C, Major).frets,
            [None, Some(3), Some(2), Some(0), Some(1), Some(0)]
        );
        assert_eq!(
            chord_shape(G, Major).frets,
            [Some(3), Some(2), Some(0), Some(0), Some(0), Some(3)]
        );
        assert_eq!(
            chord_shape(D, Dominant7).frets,
            [None, None, Some(0), Some(2), Some(1), Some(2)]
        );
        assert_eq!(
            chord_shape(A, Minor).frets,
            [None, Some(0), Some(2), Some(2), Some(1), Some(0)]
        );
        assert_eq!(
            chord_shape(E, Major).frets,
            [Some(0), Some(2), Some(2), Some(1), Some(0), Some(0)]
        );
    }

    #[test]
    fn barre_preference_picks_the_lower_position() {
        use ChordQuality::*;
        use ChordRoot::*;
        // B major is classically an A-shape barre at fret 2, not E-shape at 7.
        assert_eq!(
            chord_shape(B, Major).frets,
            [None, Some(2), Some(4), Some(4), Some(4), Some(2)]
        );
        // F# major is classically an E-shape barre at fret 2, not A-shape at 9.
        assert_eq!(
            chord_shape(Fs, Major).frets,
            [Some(2), Some(4), Some(4), Some(3), Some(2), Some(2)]
        );
        // C minor has no open shape: A-shape barre at fret 3 beats E-shape at 8.
        assert_eq!(
            chord_shape(C, Minor).frets,
            [None, Some(3), Some(5), Some(5), Some(4), Some(3)]
        );
    }

    #[test]
    fn engine_notes_reverse_string_order() {
        use ChordQuality::*;
        use ChordRoot::*;
        // C major low → high [x,3,2,0,1,0] becomes engine order
        // (index 0 = high E): high E open, B string fret 1, G open,
        // D fret 2, A fret 3, low E muted.
        let notes = chord_shape(C, Major).engine_notes();
        assert_eq!(notes, vec![(0, 0), (1, 1), (2, 0), (3, 2), (4, 3)]);
    }

    #[test]
    fn fingering_text_is_space_separated() {
        use ChordQuality::*;
        use ChordRoot::*;
        assert_eq!(chord_shape(C, Major).fingering_text(), "x 3 2 0 1 0");
        assert_eq!(chord_shape(B, Major).fingering_text(), "x 2 4 4 4 2");
        // A# minor has no open shape: A-shape barre at fret 1 wins over E-shape at 6.
        assert_eq!(chord_shape(As, Minor).fingering_text(), "x 1 3 3 2 1");
    }

    #[test]
    fn names_and_indices_round_trip() {
        assert_eq!(ChordShape::name(ChordRoot::Fs, ChordQuality::Minor7), "F#m7");
        assert_eq!(ChordShape::name(ChordRoot::C, ChordQuality::Major), "C");
        assert_eq!(ChordRoot::from_index(12), ChordRoot::C);
        assert_eq!(ChordRoot::from_index(-1), ChordRoot::B);
        assert_eq!(ChordQuality::from_index(5), ChordQuality::Major);
    }
}
