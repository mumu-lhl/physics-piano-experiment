//! [VIZIA](https://github.com/vizia/vizia) editor support for NIH plug.

// See the comment in the main `nice_plug` crate
#![allow(clippy::type_complexity)]

use crossbeam::atomic::AtomicCell;
use nice_plug_core::context::gui::GuiContext;
use nice_plug_core::params::persist::PersistentField;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use vizia::prelude::*;

// Re-export for convenience
pub use vizia;

mod editor;
pub mod widgets;

use widgets::param_registry::ParamRegistry;

/// Create an [`Editor`] instance using a [`vizia`][::vizia] GUI. The [`ViziaState`] passed to this
/// function contains the GUI's intitial size, and this is kept in sync whenever the GUI gets
/// resized. You can also use this to know if the GUI is open, so you can avoid performing
/// potentially expensive calculations while the GUI is not open. If you want this size to be
/// persisted when restoring a plugin instance, then you can store it in a `#[persist = "key"]`
/// field on your parameters struct.
///
/// The [`GuiContext`] is also passed to the app function. This is only meant for saving and
/// restoring state as part of your plugin's preset handling. You should not interact with this
/// directly to set parameters. Use the [`ParamEvent`][widgets::ParamEvent]s to change parameter
/// values, and [`GuiContextEvent`] to trigger window resizes.
///
/// The `theming` argument controls what level of theming to apply. If you use
/// [`ViziaTheming::Custom`], then you **need** to call
/// [`vizia_plug::assets::register_noto_sans_light()`][assets::register_noto_sans_light()] at
/// the start of your app function. Vizia's included fonts are also not registered by default. If
/// you use the Roboto font that normally comes with Vizia or any of its emoji or icon fonts, you
/// also need to register those using the functions in
/// [`vizia_plug::vizia_assets`][crate::vizia_assets].
///
pub use editor::ViziaEditor;

/// Create an [`Editor`] instance using a [`vizia`][::vizia] GUI.
pub fn create_vizia_editor<F>(
    vizia_state: Arc<ViziaState>,
    theming: ViziaTheming,
    app: F,
) -> Option<ViziaEditor>
where
    F: Fn(&mut Context, GuiContext) + 'static + Send + Sync,
{
    Some(editor::ViziaEditor {
        vizia_state,
        app: Arc::new(app),
        theming,

        #[cfg(target_os = "macos")]
        scaling_factor: AtomicCell::new(None),
        #[cfg(not(target_os = "macos"))]
        scaling_factor: AtomicCell::new(Some(1.0)),

        emit_parameters_changed_event: Arc::new(AtomicBool::new(false)),
        param_registry: ParamRegistry::new(),
        key_inject: editor::KeyInjectState::new(),
    })
}

/// Controls what level of theming to apply to the editor.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Default)]
pub enum ViziaTheming {
    /// Disable both `vizia_plug`'s and vizia's built-in theming.
    None,
    /// Disable `vizia_plug`'s custom theming. Vizia's included fonts are also not registered by
    /// default. If you use the Roboto font that normally comes with Vizia or any of its emoji or
    /// icon fonts, you need to register those using the functions in
    /// [`vizia_plug::vizia_assets`][crate::vizia_assets].
    Builtin,
    /// Apply `vizia_plug`'s custom theming. This is the default. You **need** to call
    /// [`vizia_plug::assets::register_noto_sans_light()`][assets::register_noto_sans_light()]
    /// at the start of your app function for the font to work correctly.
    #[default]
    Custom,
}

/// State for an `vizia_plug` editor. The scale factor can be manipulated at runtime using
/// `cx.set_user_scale_factor()`.
#[derive(Serialize, Deserialize)]
pub struct ViziaState {
    /// A function that returns the window's current size in logical pixels, before any sort of
    /// scaling is applied. This size can be computed based on the plugin's current state.
    #[serde(skip, default = "empty_size_fn")]
    size_fn: Box<dyn Fn() -> (u32, u32) + Send + Sync>,
    /// A scale factor that should be applied to `size` separate from from any system HiDPI scaling.
    /// This can be used to allow GUIs to be scaled uniformly.
    #[serde(with = "nice_plug_core::params::persist::serialize_atomic_cell")]
    scale_factor: AtomicCell<f64>,
    /// Reactive mirror of `scale_factor`, so views (e.g. the zoom stepper) stay in sync when the
    /// scale changes from anywhere: the stepper, a host resize, or a dragged window edge. Not
    /// persisted itself; it is re-synced from `scale_factor` whenever state is restored.
    #[serde(skip, default = "default_scale_signal")]
    scale_signal: SyncSignal<f64>,
    /// Whether the light theme is active. Persisted; the dark theme is the default.
    #[serde(with = "nice_plug_core::params::persist::serialize_atomic_cell")]
    light_theme: AtomicCell<bool>,
    /// Reactive mirror of `light_theme`, so views (the theme toggle button and the root
    /// container's `theme-light` class) stay in sync. Not persisted itself; it is re-synced
    /// from `light_theme` whenever state is restored.
    #[serde(skip, default = "default_theme_signal")]
    theme_signal: SyncSignal<bool>,
    /// Whether the editor's window is currently open.
    #[serde(skip)]
    open: AtomicBool,
}

/// A default implementation for `size_fn` needed to be able to derive the `Deserialize` trait.
fn empty_size_fn() -> Box<dyn Fn() -> (u32, u32) + Send + Sync> {
    Box::new(|| (0, 0))
}

/// Default for the skipped `scale_signal` field on deserialize; re-synced from `scale_factor`
/// by the `PersistentField` implementation when state is restored.
fn default_scale_signal() -> SyncSignal<f64> {
    SyncSignal::new(1.0)
}

/// Default for the skipped `theme_signal` field on deserialize; re-synced from `light_theme`
/// by the `PersistentField` implementation when state is restored.
fn default_theme_signal() -> SyncSignal<bool> {
    SyncSignal::new(false)
}

impl Debug for ViziaState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (width, height) = (self.size_fn)();

        f.debug_struct("ViziaState")
            .field("size_fn", &format!("<fn> ({}, {})", width, height))
            .field("scale_factor", &self.scale_factor)
            .field("light_theme", &self.light_theme)
            .field("open", &self.open)
            .finish()
    }
}

impl<'a> PersistentField<'a, ViziaState> for Arc<ViziaState> {
    fn set(&self, new_value: ViziaState) {
        self.set_user_scale_factor(new_value.scale_factor.load());
        self.set_light_theme(new_value.light_theme.load());
    }

    fn map<F, R>(&self, f: F) -> R
    where
        F: Fn(&ViziaState) -> R,
    {
        f(self)
    }
}

impl ViziaState {
    /// Initialize the GUI's state. This value can be passed to [`create_vizia_editor()`]. The
    /// callback always returns the window's current size is in logical pixels, so before it is
    /// multiplied by the DPI scaling factor. This size can be computed based on the plugin's
    /// current state.
    pub fn new(size_fn: impl Fn() -> (u32, u32) + Send + Sync + 'static) -> Arc<ViziaState> {
        Arc::new(ViziaState {
            size_fn: Box::new(size_fn),
            scale_factor: AtomicCell::new(1.0),
            scale_signal: SyncSignal::new(1.0),
            light_theme: AtomicCell::new(false),
            theme_signal: SyncSignal::new(false),
            open: AtomicBool::new(false),
        })
    }

    /// The same as [`new()`][Self::new()], but with a separate initial scale factor. This scale
    /// factor gets applied on top of any HiDPI scaling, and it can be modified at runtime by
    /// changing `cx.set_user_scale_factor()`.
    pub fn new_with_default_scale_factor(
        size_fn: impl Fn() -> (u32, u32) + Send + Sync + 'static,
        default_scale_factor: f64,
    ) -> Arc<ViziaState> {
        Arc::new(ViziaState {
            size_fn: Box::new(size_fn),
            scale_factor: AtomicCell::new(default_scale_factor),
            scale_signal: SyncSignal::new(default_scale_factor),
            light_theme: AtomicCell::new(false),
            theme_signal: SyncSignal::new(false),
            open: AtomicBool::new(false),
        })
    }

    /// Returns a `(width, height)` pair for the current size of the GUI in logical pixels, after
    /// applying the user scale factor.
    pub fn scaled_logical_size(&self) -> (u32, u32) {
        let (logical_width, logical_height) = self.inner_logical_size();
        let scale_factor = self.scale_factor.load();

        (
            (logical_width as f64 * scale_factor).round() as u32,
            (logical_height as f64 * scale_factor).round() as u32,
        )
    }

    /// Returns a `(width, height)` pair for the current size of the GUI in logical pixels before
    /// applying the user scale factor.
    pub fn inner_logical_size(&self) -> (u32, u32) {
        (self.size_fn)()
    }

    /// Get the non-DPI related uniform scaling factor the GUI's size will be multiplied with. This
    /// can be changed by changing `cx.user_scale_factor`.
    pub fn user_scale_factor(&self) -> f64 {
        self.scale_factor.load()
    }

    /// Set the non-DPI related uniform scaling factor the GUI's size will be multiplied with.
    ///
    /// `Editor::size()` reads this via [`Self::scaled_logical_size`], so updating it before
    /// calling [`nice_plug::context::gui::GuiContext::request_resize`] makes the host see the
    /// freshly-zoomed dimensions and resize its parent window to match.
    ///
    /// On its own, this only changes what the host is told. To make the embedded vizia window
    /// actually re-lay-out at the new scale, emit
    /// [`vizia::prelude::WindowEvent::SetUserScale`] from the same handler. The
    /// `vizia_baseview` backend handles that event by resizing the embedded window, bumping
    /// `style.dpi_factor`, and refreshing the render surface in lockstep.
    pub fn set_user_scale_factor(&self, factor: f64) {
        self.scale_factor.store(factor);
        self.scale_signal.set(factor);
    }

    /// Reactive mirror of the user scale factor. Views bind to this instead of snapshotting
    /// [`Self::user_scale_factor`] so they track scale changes from every source.
    pub fn scale_signal(&self) -> SyncSignal<f64> {
        self.scale_signal.clone()
    }

    /// Whether the light theme is active. The dark theme is the default.
    pub fn light_theme(&self) -> bool {
        self.light_theme.load()
    }

    /// Set the active theme. Views bound to [`Self::theme_signal`] (the theme toggle button
    /// label and the root container's `theme-light` class) update immediately, and the
    /// choice is persisted with the plugin state.
    pub fn set_light_theme(&self, light: bool) {
        self.light_theme.store(light);
        self.theme_signal.set(light);
    }

    /// Reactive mirror of the active theme. Views bind to this instead of snapshotting
    /// [`Self::light_theme`] so they track theme changes from every source.
    pub fn theme_signal(&self) -> SyncSignal<bool> {
        self.theme_signal.clone()
    }

    /// Whether the GUI is currently visible.
    // Called `is_open()` instead of `open()` to avoid the ambiguity.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_defaults_to_dark() {
        let state = ViziaState::new(|| (800, 600));
        assert!(!state.light_theme());
        assert!(!state.theme_signal().get());
    }

    #[test]
    fn set_light_theme_syncs_value_and_signal() {
        let state = ViziaState::new(|| (800, 600));
        state.set_light_theme(true);
        assert!(state.light_theme());
        assert!(state.theme_signal().get());
        state.set_light_theme(false);
        assert!(!state.light_theme());
        assert!(!state.theme_signal().get());
    }

    #[test]
    fn restored_state_resyncs_theme_signal() {
        use nice_plug_core::params::persist::PersistentField;

        let state = ViziaState::new(|| (800, 600));
        let restored = ViziaState::new(|| (800, 600));
        restored.set_light_theme(true);
        let restored = Arc::try_unwrap(restored).unwrap();
        state.set(restored);
        assert!(state.light_theme());
        assert!(state.theme_signal().get());
    }
}
