//! Apply a stored preset's plain values to a plugin's parameters
//! without going through the GUI event path.
//!
//! Used once, at plugin-instance construction, to load the launch
//! factory preset (P0-1): the editor names a preset from the moment it
//! opens, so the parameters must already hold that preset's values.
//! The GUI preset path (`set_param`) cannot run there — there is no
//! event context yet — so this helper performs the same conversion
//! directly: each plain value is turned into a normalized value with
//! [`ParamPtr::preview_normalized`] and written with
//! [`ParamPtr::_internal_set_normalized_value`], exactly the two steps
//! `ParamEvent::SetParameter` performs on its way through vizia-plug's
//! `ParamModel`. Two conversions need the GUI handlers' own
//! pre-steps to agree exactly: integer values are rounded first (the
//! handlers call `value.round() as i32`, while `preview_normalized`
//! truncates with `as i32`), and boolean values use the handlers'
//! `>= 0.5` threshold (the bool preview passes the value through and
//! the plain conversion tests `> 0.5`). With those aligned, applying
//! a preset here and applying it from the GUI land on byte-identical
//! plain values, including integer rounding and range clamping.

use std::collections::HashMap;

use nice_plug::prelude::ParamPtr;
use physics_presets::Preset;

/// Tolerance for "already at the preset value", matching the GUI
/// preset handlers, which skip a parameter when its current plain
/// value is within `1e-5` of the preset value.
const UNCHANGED_TOLERANCE: f32 = 1e-5;

/// Apply `preset`'s plain values to the parameters in `param_map`,
/// returning the IDs that actually changed, sorted for stable
/// diagnostics. Entries whose ID is not a parameter (including legacy
/// entries the caller did not migrate, e.g. piano's `velocity_curve`)
/// and non-finite values are skipped, mirroring the GUI handlers,
/// which ignore unknown IDs the same way.
///
/// # Safety
///
/// The caller guarantees the parameters `param_map` points into
/// outlive this call. This holds at plugin construction, where the
/// map is built from the `Arc` the plugin keeps for its lifetime —
/// the same contract [`crate::apply_midi_cc`] relies on.
pub fn apply_preset_values(param_map: &HashMap<String, ParamPtr>, preset: &Preset) -> Vec<String> {
    let mut applied = Vec::new();
    for (id, &plain) in &preset.params {
        if !plain.is_finite() {
            continue;
        }
        let Some(ptr) = param_map.get(id) else {
            continue;
        };
        // SAFETY: the caller guarantees the params outlive this call.
        let current = unsafe { ptr.unmodulated_plain_value() };
        if (current - plain).abs() <= UNCHANGED_TOLERANCE {
            continue;
        }
        // Align the conversion with the GUI handlers before previewing:
        // round integer values (the handlers round, `preview_normalized`
        // truncates) and apply the handlers' `>= 0.5` boolean threshold
        // (the bool preview passes the value through unchanged).
        let normalized = unsafe {
            match ptr {
                ParamPtr::IntParam(_) | ParamPtr::EnumParam(_) => {
                    ptr.preview_normalized(plain.round())
                }
                ParamPtr::BoolParam(_) => {
                    ptr.preview_normalized(if plain >= 0.5 { 1.0 } else { 0.0 })
                }
                ParamPtr::FloatParam(_) => ptr.preview_normalized(plain),
            }
        };
        // SAFETY: same guarantee as above. This is the conversion the
        // GUI path performs, so no host-automation gesture, undo
        // record, or smoother update is involved here either.
        unsafe {
            ptr._internal_set_normalized_value(normalized);
        }
        applied.push(id.clone());
    }
    applied.sort();
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_param_map;
    use nice_plug::prelude::*;

    #[derive(Params)]
    struct TestParams {
        #[id = "tone"]
        pub tone: FloatParam,
        #[id = "pickup_pos"]
        pub pickup_pos: IntParam,
        #[id = "cab_enabled"]
        pub cab_enabled: BoolParam,
    }

    impl Default for TestParams {
        fn default() -> Self {
            Self {
                tone: FloatParam::new("Tone", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
                pickup_pos: IntParam::new("Pickup", 0, IntRange::Linear { min: 0, max: 4 }),
                cab_enabled: BoolParam::new("Cab", false),
            }
        }
    }

    fn fixture() -> (Box<TestParams>, HashMap<String, ParamPtr>) {
        // The map holds raw pointers into the params: box first so the
        // pointee never moves after the map is built (moving the struct
        // would dangle every pointer and segfault on use).
        let params = Box::new(TestParams::default());
        let map = build_param_map(&*params);
        (params, map)
    }

    fn preset_with(pairs: &[(&str, f32)]) -> Preset {
        Preset::new(
            "test",
            "Test",
            "test",
            pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        )
    }

    #[test]
    fn applies_float_int_and_bool_values() {
        let (params, map) = fixture();
        let applied = apply_preset_values(
            &map,
            &preset_with(&[("tone", 0.95), ("pickup_pos", 4.0), ("cab_enabled", 1.0)]),
        );
        assert_eq!(applied, vec!["cab_enabled", "pickup_pos", "tone"]);
        assert!((params.tone.value() - 0.95).abs() < 1e-6);
        assert_eq!(params.pickup_pos.value(), 4);
        assert!(params.cab_enabled.value());
    }

    #[test]
    fn reapplying_the_same_preset_changes_nothing() {
        let (params, map) = fixture();
        let preset = preset_with(&[("tone", 0.95), ("pickup_pos", 4.0)]);
        assert_eq!(apply_preset_values(&map, &preset).len(), 2);
        assert!(apply_preset_values(&map, &preset).is_empty());
        assert!((params.tone.value() - 0.95).abs() < 1e-6);
    }

    #[test]
    fn skips_unknown_ids_and_values_already_at_preset() {
        let (params, map) = fixture();
        let applied = apply_preset_values(
            &map,
            &preset_with(&[("tone", 0.5), ("nope", 1.0), ("velocity_curve", 0.0)]),
        );
        assert!(applied.is_empty());
        assert!((params.tone.value() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn clamps_out_of_range_plain_values_to_the_range_edge() {
        let (params, map) = fixture();
        apply_preset_values(&map, &preset_with(&[("tone", 2.0), ("pickup_pos", -3.0)]));
        assert!((params.tone.value() - 1.0).abs() < 1e-6);
        assert_eq!(params.pickup_pos.value(), 0);
    }

    #[test]
    fn rounds_fractional_int_values_like_the_gui_path() {
        let (params, map) = fixture();
        // The GUI handlers round with `value.round() as i32`; the
        // normalized round-trip must agree, or navigating away from
        // and back to a preset would land on a different value.
        apply_preset_values(&map, &preset_with(&[("pickup_pos", 2.7)]));
        assert_eq!(params.pickup_pos.value(), 3);
    }

    #[test]
    fn bool_threshold_matches_the_gui_path_at_exactly_half() {
        let (params, map) = fixture();
        // The GUI handlers use `value >= 0.5`; the bool plain
        // conversion tests `> 0.5`, so 0.5 must be thresholded first.
        apply_preset_values(&map, &preset_with(&[("cab_enabled", 0.5)]));
        assert!(params.cab_enabled.value());
        apply_preset_values(&map, &preset_with(&[("cab_enabled", 0.49)]));
        assert!(!params.cab_enabled.value());
    }

    #[test]
    fn skips_non_finite_values() {
        let (params, map) = fixture();
        apply_preset_values(
            &map,
            &preset_with(&[("tone", f32::NAN), ("pickup_pos", f32::INFINITY)]),
        );
        assert!((params.tone.value() - 0.5).abs() < 1e-6);
        assert_eq!(params.pickup_pos.value(), 0);
    }
}
