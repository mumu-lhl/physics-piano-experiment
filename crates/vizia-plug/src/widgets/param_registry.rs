//! Bridge between nice-plug's pull-based [`Param`] model and vizia's push-based
//! [`SyncSignal`] reactive graph.
//!
//! nice-plug exposes parameters through [`ParamPtr`] — stable opaque handles whose current
//! values are read on demand via unsafe accessors. vizia's new signal-based binding system
//! (vizia#619) requires observable values to be wrapped in [`SyncSignal`] so the reactive
//! graph can track dependencies and push updates to subscribers.
//!
//! [`ParamRegistry`] owns one [`SyncSignal<f32>`] per `(ParamPtr, axis)` pair (axes:
//! `Modulated`, `Unmodulated`). Widgets call
//! [`ParamRegistry::modulated`] / [`ParamRegistry::unmodulated`] on construction to obtain a
//! signal for the param value they care about; the registry lazily creates signals on first
//! access and reuses them on subsequent accesses.
//!
//! The editor side is responsible for flushing current values from [`ParamPtr`]s into the
//! registry's signals whenever nice-plug reports a parameter change (via
//! [`Editor::parameter_value_changed`](nice_plug::prelude::Editor::parameter_value_changed) /
//! [`Editor::parameter_values_changed`](nice_plug::prelude::Editor::parameter_values_changed)).
//! See [`ParamRegistry::flush_all`].
//!
//! The type is cheaply `Clone` (it's an `Arc` internally), so the editor can keep its own
//! handle for flushing while also installing a clone as a vizia [`Model`] for widget lookup.

use std::collections::HashMap;
use std::sync::Arc;

use nice_plug_core::params::internals::ParamPtr;
use parking_lot::Mutex;
use vizia::prelude::*;

/// Which value of a parameter a signal tracks. nice-plug distinguishes between the raw
/// user/host-set value (*unmodulated*) and the value after any monophonic modulation has been
/// applied (*modulated*). Most widgets want modulated — it's what the user sees driving the
/// audio — but some (e.g. a slider that visualises both) want both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamAxis {
    /// `ParamPtr::modulated_normalized_value()`.
    Modulated,
    /// `ParamPtr::unmodulated_normalized_value()`.
    Unmodulated,
}

/// Shared, `Clone`-able handle to a set of param-tracking [`SyncSignal`]s. The same value
/// backs both the editor's flush path and the widget-facing lookup path — cloning a
/// `ParamRegistry` returns another handle to the same underlying signal map.
#[derive(Clone)]
pub struct ParamRegistry {
    inner: Arc<ParamRegistryInner>,
}

struct ParamRegistryInner {
    /// Lazily populated map of `(ParamPtr, axis)` → signal.
    ///
    /// Locked briefly by widgets on construction (UI thread) and by the editor on every
    /// parameter-change callback ([`flush_all`](ParamRegistry::flush_all), which nice-plug
    /// calls on the host / audio thread). Using `parking_lot::Mutex` rather than
    /// `std::sync::Mutex` keeps the audio-thread side light — no poisoning checks, no
    /// priority-inversion hazard if the UI thread is holding the lock during widget build.
    signals: Mutex<HashMap<(ParamPtr, ParamAxis), SyncSignal<f32>>>,
}

impl ParamRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ParamRegistryInner {
                signals: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Drop all cached signals.
    ///
    /// Signals are tied to the editor's reactive runtime. When an editor instance closes, those
    /// runtime-owned signals become invalid and must not be reused after reopening.
    pub fn clear_signals(&self) {
        self.inner.signals.lock().clear();
    }

    /// Returns the signal tracking `param_ptr`'s value on the given `axis`, creating it
    /// (initialised from the current unsafe `ParamPtr` value) if it does not yet exist.
    pub fn signal(&self, param_ptr: ParamPtr, axis: ParamAxis) -> SyncSignal<f32> {
        let mut signals = self.inner.signals.lock();

        *signals.entry((param_ptr, axis)).or_insert_with(|| {
            // SAFETY: `param_ptr` was resolved from a valid `&impl Param` at widget
            // construction; it stays valid for the plugin's lifetime.
            let initial = unsafe {
                match axis {
                    ParamAxis::Modulated => param_ptr.modulated_normalized_value(),
                    ParamAxis::Unmodulated => param_ptr.unmodulated_normalized_value(),
                }
            };
            SyncSignal::new(initial)
        })
    }

    /// Shorthand for the common case: the modulated normalised value.
    pub fn modulated(&self, param_ptr: ParamPtr) -> SyncSignal<f32> {
        self.signal(param_ptr, ParamAxis::Modulated)
    }

    /// Shorthand for the unmodulated (user/host-set) value.
    pub fn unmodulated(&self, param_ptr: ParamPtr) -> SyncSignal<f32> {
        self.signal(param_ptr, ParamAxis::Unmodulated)
    }

    /// Re-read every registered parameter via unsafe `ParamPtr` and write the current value
    /// into its signal. Intended to be called from the editor's
    /// [`parameter_values_changed`](nice_plug::prelude::Editor::parameter_values_changed) hook
    /// (which reports "something moved but we don't know what") so every widget catches up
    /// in one sweep.
    ///
    /// The full sweep is load-bearing and must NOT be replaced by a targeted refresh on the
    /// 33 ms `RefreshDisplay` tick (see the three editors): there is no per-binding dirty
    /// tracking anywhere in the stack, so at tick time nothing records *which* parameter
    /// moved — discovering that is exactly what this sweep does, by re-reading every
    /// `ParamPtr` and letting [`SyncSignal::set_if_changed`] drop the unchanged ones. The
    /// writers that bypass notification are real: the audio thread applies learned MIDI CCs
    /// with `ParamPtr::_internal_set_normalized_value` (no task is posted, no callback
    /// fires), and host/GUI writes round-trip through nice-plug's `GuiContext` without
    /// leaving a dirty set this crate could consult. A targeted refresh would need dirty
    /// instrumentation at every writer across all three instruments plus the nice-plug
    /// boundary; any writer missed reintroduces stale knobs (the P1-13 failure mode the
    /// sweep exists to prevent). The cost is one atomic read and one float compare per
    /// registered signal per tick — negligible against a missed refresh. Use
    /// [`flush_one`](Self::flush_one) only where the changed parameter is known, i.e. the
    /// host-driven `parameter_value_changed` / `parameter_modulation_changed` callbacks,
    /// which keep their full sweep too because they receive an id with no `ParamPtr`
    /// lookup in this crate.
    pub fn flush_all(&self) {
        let signals = self.inner.signals.lock();

        for ((param_ptr, axis), signal) in signals.iter() {
            // SAFETY: see `signal()`.
            let current = unsafe {
                match axis {
                    ParamAxis::Modulated => param_ptr.modulated_normalized_value(),
                    ParamAxis::Unmodulated => param_ptr.unmodulated_normalized_value(),
                }
            };
            signal.set_if_changed(current);
        }
    }

    /// Re-read a single parameter via unsafe `ParamPtr` and write its current value into each
    /// of its registered axis signals (modulated + unmodulated). Intended to be called from
    /// the editor's
    /// [`parameter_value_changed`](nice_plug::prelude::Editor::parameter_value_changed) /
    /// [`parameter_modulation_changed`](nice_plug::prelude::Editor::parameter_modulation_changed)
    /// hooks — both of which report the specific parameter that moved — so a single-parameter
    /// change doesn't iterate every signal in the registry.
    ///
    /// Axes with no signal registered for this parameter are silently skipped.
    pub fn flush_one(&self, param_ptr: ParamPtr) {
        let signals = self.inner.signals.lock();

        for axis in [ParamAxis::Modulated, ParamAxis::Unmodulated] {
            if let Some(signal) = signals.get(&(param_ptr, axis)) {
                // SAFETY: see `signal()`.
                let current = unsafe {
                    match axis {
                        ParamAxis::Modulated => param_ptr.modulated_normalized_value(),
                        ParamAxis::Unmodulated => param_ptr.unmodulated_normalized_value(),
                    }
                };
                signal.set_if_changed(current);
            }
        }
    }
}

impl Default for ParamRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Model for ParamRegistry {}

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug_core::params::range::FloatRange;
    use nice_plug_core::params::{FloatParam, Param};

    struct TestParams {
        pub tone: FloatParam,
        pub gain: FloatParam,
    }

    /// Boxed params plus their `ParamPtr`s, mirroring the MIDI Learn
    /// fixture: the pointers borrow the params, so the params must
    /// never move after the pointers are taken. (No `Params` derive:
    /// the derive macro hardcodes the `nice-plug` facade crate, which
    /// this crate does not depend on; `Param::as_ptr` is enough.)
    fn fixture() -> (Box<TestParams>, ParamPtr, ParamPtr) {
        let params = Box::new(TestParams {
            tone: FloatParam::new("Tone", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
            gain: FloatParam::new("Gain", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
        });
        let tone = params.tone.as_ptr();
        let gain = params.gain.as_ptr();
        (params, tone, gain)
    }

    /// The MIDI CC pattern: a direct `ParamPtr` write with no
    /// notification of any kind. `flush_all` — the 33 ms tick's
    /// refresh — must still surface the new value in the signal.
    #[test]
    fn flush_all_picks_up_direct_param_writes() {
        let (_params, tone, _gain) = fixture();
        let registry = ParamRegistry::new();
        let signal = registry.unmodulated(tone);
        assert_eq!(signal.get(), 0.5);

        // SAFETY: the boxed params outlive this test; mirrors
        // `apply_midi_cc`'s direct write on the audio thread.
        unsafe {
            assert!(tone._internal_set_normalized_value(0.25));
        }
        // The signal is stale until a flush happens: nothing notified it.
        assert_eq!(signal.get(), 0.5);

        registry.flush_all();
        assert_eq!(signal.get(), 0.25);
    }

    /// `flush_one` is targeted: flushing the wrong parameter leaves
    /// the moved parameter's signal stale. That staleness is exactly
    /// why the 33 ms tick cannot use `flush_one` — at tick time the
    /// moved parameter is unknown, so only `flush_all` is safe.
    #[test]
    fn flush_one_leaves_other_params_untouched() {
        let (_params, tone, gain) = fixture();
        let registry = ParamRegistry::new();
        let tone_signal = registry.unmodulated(tone);
        let gain_signal = registry.unmodulated(gain);

        // SAFETY: see above.
        unsafe {
            assert!(gain._internal_set_normalized_value(0.75));
        }
        registry.flush_one(tone);
        assert_eq!(tone_signal.get(), 0.5);
        assert_eq!(gain_signal.get(), 0.5);

        registry.flush_all();
        assert_eq!(gain_signal.get(), 0.75);
    }
}
