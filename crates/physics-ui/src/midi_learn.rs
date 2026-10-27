//! MIDI Learn helpers shared by the piano, guitar and bass editors.
//!
//! The audio thread applies learned bindings with [`apply_midi_cc`]; the
//! editors arm knobs for learning with [`LearnArm`], consulted by
//! `parameter_knob`.
//!
//! Threading split is deliberate: [`LearnArm`] holds vizia `Signal`s, which
//! are GUI-thread handles and must never cross to the audio thread. The
//! audio thread only touches the `PresetManager` (behind a `parking_lot`
//! `RwLock`, shared with the editor) and the last-CC atomics owned by the
//! plugin struct.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use nice_plug::prelude::{ParamPtr, Params};
use vizia_plug::vizia::prelude::*;

/// Build the parameter-id -> `ParamPtr` lookup for a plugin's params.
/// Built once at plugin construction so the audio thread never allocates
/// while handling CC events.
///
/// The returned pointers borrow the params: build the map only after the
/// params are in their final location (e.g., inside the `Arc` the plugin
/// keeps for its lifetime). Moving the params afterwards invalidates every
/// pointer in the map.
pub fn build_param_map<P: Params>(params: &P) -> HashMap<String, ParamPtr> {
    params
        .param_map()
        .into_iter()
        .map(|(id, ptr, _)| (id, ptr))
        .collect()
}

/// Build the reverse lookup (`ParamPtr` -> parameter id) for the editor's
/// Learn arming.
pub fn build_ptr_to_id_map<P: Params>(params: &P) -> HashMap<ParamPtr, String> {
    params
        .param_map()
        .into_iter()
        .map(|(id, ptr, _)| (ptr, id))
        .collect()
}

/// Apply one learned MIDI CC to its bound parameter, if any. Returns the
/// bound parameter id when a mapping existed and was applied.
///
/// # Safety
/// The caller guarantees the params outlive this call. This holds inside
/// `process()`, where the plugin owns its params for its whole lifetime.
pub fn apply_midi_cc(
    param_map: &HashMap<String, ParamPtr>,
    mappings: &HashMap<u8, String>,
    cc: u8,
    value: f32,
) -> Option<String> {
    let id = mappings.get(&cc)?;
    let ptr = param_map.get(id)?;
    // SAFETY: params are owned by the plugin for its whole lifetime.
    unsafe {
        ptr._internal_set_normalized_value(value.clamp(0.0, 1.0));
    }
    Some(id.clone())
}

/// GUI-thread Learn state shared between the Learn button, the knob arming
/// hook in `parameter_knob`, and the editor's display timer.
#[derive(Clone)]
pub struct LearnArm {
    /// Whether Learn mode is on (Learn button lit).
    pub mode: Signal<bool>,
    /// Parameter id armed by the last knob click while Learn was on.
    pub pending: Signal<Option<String>>,
    /// `ParamPtr` -> parameter id, built once from `param_map()`.
    pub ptr_to_id: Arc<HashMap<ParamPtr, String>>,
}

impl LearnArm {
    pub fn new(ptr_to_id: HashMap<ParamPtr, String>) -> Self {
        Self {
            mode: Signal::new(false),
            pending: Signal::new(None),
            ptr_to_id: Arc::new(ptr_to_id),
        }
    }
}

/// Last MIDI CC received, shared lock-free between the audio thread
/// (records) and the editor's display timer (polls for Learn binding).
pub struct LastCc {
    cc: AtomicU8,
    seq: AtomicU32,
}

impl LastCc {
    pub fn new() -> Self {
        Self {
            cc: AtomicU8::new(255),
            seq: AtomicU32::new(0),
        }
    }

    /// Record a received CC. The sequence bump (Release) publishes the CC
    /// store to the polling GUI thread.
    pub fn record(&self, cc: u8) {
        self.cc.store(cc, Ordering::Relaxed);
        self.seq.fetch_add(1, Ordering::Release);
    }

    pub fn cc(&self) -> u8 {
        self.cc.load(Ordering::Relaxed)
    }

    /// Sequence counter; the GUI thread loads with Acquire, which pairs
    /// with the Release in `record`, so a later `cc()` read sees the
    /// recorded value.
    pub fn seq(&self) -> u32 {
        self.seq.load(Ordering::Acquire)
    }
}

impl Default for LastCc {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::prelude::*;

    #[derive(Params)]
    struct TestParams {
        #[id = "tone"]
        pub tone: FloatParam,
        #[id = "gain"]
        pub gain: FloatParam,
    }

    impl Default for TestParams {
        fn default() -> Self {
            Self {
                tone: FloatParam::new("Tone", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
                gain: FloatParam::new("Gain", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
            }
        }
    }

    fn fixture() -> (
        Box<TestParams>,
        HashMap<String, ParamPtr>,
        HashMap<u8, String>,
    ) {
        // The map holds raw pointers into the params: box first so the
        // pointee never moves after the map is built (moving the struct
        // would dangle every pointer and segfault on use).
        let params = Box::new(TestParams::default());
        let map = build_param_map(&*params);
        let mut mappings = HashMap::new();
        mappings.insert(7, "tone".to_string());
        (params, map, mappings)
    }

    #[test]
    fn apply_mapped_cc_sets_param_and_returns_id() {
        let (params, map, mappings) = fixture();
        assert_eq!(
            apply_midi_cc(&map, &mappings, 7, 0.25),
            Some("tone".to_string())
        );
        assert!((params.tone.value() - 0.25).abs() < 1e-6);
        assert!((params.gain.value() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn apply_unmapped_cc_returns_none_and_changes_nothing() {
        let (params, map, mappings) = fixture();
        assert_eq!(apply_midi_cc(&map, &mappings, 8, 0.9), None);
        assert!((params.tone.value() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn apply_stale_id_returns_none() {
        let (params, map, mut mappings) = fixture();
        mappings.insert(9, "nope".to_string());
        assert_eq!(apply_midi_cc(&map, &mappings, 9, 0.9), None);
        assert!((params.tone.value() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn apply_clamps_value_to_unit_range() {
        let (params, map, mappings) = fixture();
        apply_midi_cc(&map, &mappings, 7, 1.5);
        assert!((params.tone.value() - 1.0).abs() < 1e-6);
        apply_midi_cc(&map, &mappings, 7, -0.5);
        assert!((params.tone.value() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn param_maps_cover_both_directions() {
        let params = TestParams::default();
        let fwd = build_param_map(&params);
        let rev = build_ptr_to_id_map(&params);
        assert_eq!(fwd.len(), 2);
        assert_eq!(rev.len(), 2);
        let tone_ptr = fwd["tone"];
        assert_eq!(rev[&tone_ptr], "tone");
    }
}
