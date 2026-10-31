//! The [`Editor`] trait implementation for Vizia editors.

use crossbeam::atomic::AtomicCell;
use nice_plug_core::context::gui::GuiContext;
use nice_plug_core::editor::dpi::NativeSize;
use nice_plug_core::editor::{
    Editor, EditorHandle, HostMethods, Modifiers, ParentWindowHandle, ResizeHint, SizeConstraints,
    SpawnedEditor, VirtualKeyCode,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use vizia::prelude::*;
use vizia::views::TextEvent;

use crate::widgets::RawParamEvent;
use crate::widgets::param_registry::ParamRegistry;
use crate::{ViziaState, ViziaTheming, widgets};

/// Adapter to convert nice-plug-core 0.4 ParentWindowHandle into raw-window-handle 0.5 HasRawWindowHandle for vizia_baseview.
pub(crate) struct Rwh05Parent(ParentWindowHandle);

unsafe impl raw_window_handle_05::HasRawWindowHandle for Rwh05Parent {
    fn raw_window_handle(&self) -> raw_window_handle_05::RawWindowHandle {
        match self.0 {
            ParentWindowHandle::XlibWindow(window) => {
                let mut handle = raw_window_handle_05::XlibWindowHandle::empty();
                handle.window = window;
                raw_window_handle_05::RawWindowHandle::Xlib(handle)
            }
            ParentWindowHandle::XcbWindow(window) => {
                let mut handle = raw_window_handle_05::XcbWindowHandle::empty();
                handle.window = window.get();
                raw_window_handle_05::RawWindowHandle::Xcb(handle)
            }
            ParentWindowHandle::AppKitNsView(ns_view) => {
                let mut handle = raw_window_handle_05::AppKitWindowHandle::empty();
                handle.ns_view = ns_view.as_ptr();
                raw_window_handle_05::RawWindowHandle::AppKit(handle)
            }
            ParentWindowHandle::Win32Hwnd(hwnd) => {
                let mut handle = raw_window_handle_05::Win32WindowHandle::empty();
                handle.hwnd = hwnd.get() as *mut std::ffi::c_void;
                raw_window_handle_05::RawWindowHandle::Win32(handle)
            }
        }
    }
}

/// A key-down event queued by the host-thread
/// `Editor::on_virtual_key_from_host` callback, waiting for the next
/// `on_idle` tick to dispatch on the GUI thread.
pub(crate) enum KeyInject {
    Char(char),
    ControlKey(Code, Key),
}

pub(crate) struct KeyInjectState {
    pub(crate) text_focused: AtomicBool,
    pub(crate) pending: Mutex<VecDeque<KeyInject>>,
}

impl KeyInjectState {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            text_focused: AtomicBool::new(false),
            pending: Mutex::new(VecDeque::new()),
        })
    }
}

/// Derives the user scale factor implied by a resized window, shared by the
/// idle-loop free-resize path and `EditorHandle::set_size`.
///
/// `current` is the scale in force before the resize; `bounds_w`/`bounds_h`
/// are the new root bounds in backend pixels and `dpi_factor` already
/// includes the current user scale, so the bounds divided by DPI equal the
/// base size whenever the window is in a consistent state. The derived
/// scale is `current × min(width ratio, height ratio)`: the `min()` also
/// defines the aspect-ratio snap the idle loop's `SetUserScale` emission
/// performs, and means a width-only or height-only drag derives `current`
/// (no change) unless the host honors the `preserve_aspect_ratio` resize
/// hint.
///
/// Returns `current` unchanged when the inputs are degenerate (zero or
/// negative DPI/bounds/base, non-finite result) or when the derived scale
/// is within 0.002 of `current` (epsilon beats integer-pixel rounding,
/// `<= ~0.5/base`). Otherwise the result is clamped to 0.2–5.0, the same
/// range `ViziaState::set_user_scale_factor` enforces. Note the clamp
/// exceeds the zoom stepper's 2.0 maximum: dragging to 5.0× and then
/// pressing the stepper still jumps (stepper clamps to its own range),
/// unchanged by this extraction.
///
/// `set_size` calls this with `current = 1.0` and `dpi_factor = 1.0`,
/// because the host size it receives already implies the scale directly.
pub(crate) fn derive_scale(
    current: f64,
    bounds_w: f64,
    bounds_h: f64,
    dpi_factor: f64,
    base_w: f64,
    base_h: f64,
) -> f64 {
    if dpi_factor <= 0.0 || bounds_w <= 0.0 || bounds_h <= 0.0 || base_w <= 0.0 || base_h <= 0.0 {
        return current;
    }
    let derived = current
        * ((bounds_w / dpi_factor) / base_w).min((bounds_h / dpi_factor) / base_h);
    if !derived.is_finite() || (derived - current).abs() <= 0.002 {
        return current;
    }
    derived.clamp(0.2, 5.0)
}

#[derive(Clone)]
pub struct ViziaWindow {
    pub(crate) inner: Arc<Mutex<Option<WindowHandle>>>,
    #[allow(clippy::type_complexity)]
    pub(crate) open_parented_fn: Arc<Mutex<Option<Box<dyn FnOnce(&Rwh05Parent) -> WindowHandle + Send>>>>,
    pub(crate) open_blocking_fn: Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>,
}

unsafe impl Send for ViziaWindow {}

#[derive(Debug, thiserror::Error)]
pub enum ViziaEditorError {
    #[error("Editor window failed to open")]
    OpenFailed,
}

/// An [`Editor`] implementation that calls a vizia draw loop.
pub struct ViziaEditor {
    pub(crate) vizia_state: Arc<ViziaState>,
    pub(crate) app: Arc<dyn Fn(&mut Context, GuiContext) + 'static + Send + Sync>,
    pub(crate) theming: ViziaTheming,
    pub(crate) scaling_factor: AtomicCell<Option<f32>>,
    pub(crate) emit_parameters_changed_event: Arc<AtomicBool>,
    pub(crate) param_registry: ParamRegistry,
    pub(crate) key_inject: Arc<KeyInjectState>,
}

impl Editor for ViziaEditor {
    type Handle = ViziaEditorHandle;

    fn spawn(
        &self,
        parent: Option<ParentWindowHandle>,
        wait_for_parent: bool,
        fallback_scale_factor: Option<f64>,
        gui_context: GuiContext,
        _host: Option<HostMethods>,
    ) -> Result<SpawnedEditor<Self::Handle>, Box<dyn std::error::Error>> {
        let app = self.app.clone();
        let vizia_state = self.vizia_state.clone();
        let theming = self.theming;
        let param_registry = self.param_registry.clone();
        let emit_parameters_changed_event = self.emit_parameters_changed_event.clone();
        let key_inject = self.key_inject.clone();

        param_registry.clear_signals();

        let (unscaled_width, unscaled_height) = vizia_state.inner_logical_size();
        let system_scaling_factor = self
            .scaling_factor
            .load()
            .map(|s| s as f64)
            .or(fallback_scale_factor);
        let user_scale_factor = vizia_state.user_scale_factor();

        let make_application = {
            let app = app.clone();
            let vizia_state = vizia_state.clone();
            let param_registry = param_registry.clone();
            let emit_parameters_changed_event = emit_parameters_changed_event.clone();
            let key_inject = key_inject.clone();
            let gui_context = gui_context.clone();

            move || {
                let vizia_state_for_idle = vizia_state.clone();
                let mut application = Application::new(move |cx| {
                    if let Err(err) = cx.add_stylesheet(include_style!("src/assets/theme.css")) {
                        // Log and continue unstyled, like the instrument
                        // editors do for the shared theme: the stylesheet
                        // is a compile-time asset, so a failure means a
                        // CSS syntax error, which must not keep every
                        // editor from opening at all.
                        eprintln!("Failed to load stylesheet: {err:?}");
                    }
                    widgets::register_theme(cx);
                    param_registry.clone().build(cx);
                    widgets::ParamModel {
                        context: gui_context.clone(),
                    }
                    .build(cx);

                    widgets::WindowModel {
                        vizia_state: vizia_state.clone(),
                    }
                    .build(cx);

                    app(cx, gui_context.clone());
                })
                .with_scale_policy(
                    system_scaling_factor
                        .map(WindowScalePolicy::ScaleFactor)
                        .unwrap_or(WindowScalePolicy::SystemScaleFactor),
                )
                .inner_size((unscaled_width, unscaled_height))
                .user_scale_factor(user_scale_factor)
                .on_idle({
                    let emit_parameters_changed_event = emit_parameters_changed_event.clone();
                    let key_inject = key_inject.clone();
                    let vizia_state = vizia_state_for_idle.clone();
                    let applied_user_scale = Arc::new(AtomicCell::new(user_scale_factor));
                    move |cx| {
                        // Free resize: when the OS window is resized by dragging an edge, the
                        // backend updates the root bounds without changing the user scale.
                        // Derive the scale from the new bounds so the content reflows. The
                        // `SetUserScale` emission below then snaps the window back to the
                        // design aspect ratio, so the min() here also defines the snap.
                        // `GeometryChanged` cannot be used for this: it propagates `Direct`,
                        // so it only ever reaches the resized entity itself.
                        let (base_width, base_height) = vizia_state.inner_logical_size();
                        if base_width > 0 && base_height > 0 {
                            let resize_cx = EventContext::new(cx);
                            let bounds = resize_cx.cache.get_bounds(Entity::root());
                            let dpi_factor = resize_cx.scale_factor() as f64;
                            if dpi_factor > 0.0 && bounds.width() > 0.0 && bounds.height() > 0.0 {
                                // `dpi_factor` already includes the current user scale, so the
                                // root bounds divided by it is the base size whenever the
                                // window is in a consistent state. An external resize shows up
                                // as a *ratio* against that state: multiply back by the
                                // current scale. (Deriving the scale from the bounds alone
                                // collapses to 1.0 once the new scale is applied, because the
                                // backend then re-reports the resize with the updated DPI.)
                                let current = vizia_state.user_scale_factor();
                                let derived = derive_scale(
                                    current,
                                    bounds.width() as f64,
                                    bounds.height() as f64,
                                    dpi_factor,
                                    base_width as f64,
                                    base_height as f64,
                                );
                                if derived != current {
                                    vizia_state.set_user_scale_factor(derived);
                                }
                            }
                        }

                        let requested_user_scale = vizia_state.user_scale_factor();
                        let current_user_scale = applied_user_scale.load();
                        if (requested_user_scale - current_user_scale).abs() > f64::EPSILON {
                            cx.emit(WindowEvent::SetUserScale(requested_user_scale));
                            applied_user_scale.store(requested_user_scale);
                        }

                        if emit_parameters_changed_event
                            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Relaxed)
                            .is_ok()
                        {
                            cx.emit_custom(
                                Event::new(RawParamEvent::ParametersChanged)
                                    .propagate(Propagation::Subtree),
                            );
                        }

                        key_inject
                            .text_focused
                            .store(cx.focused_element() == Some("textbox"), Ordering::Release);

                        let drained: Vec<KeyInject> = {
                            let mut q =
                                key_inject.pending.lock().unwrap_or_else(|e| e.into_inner());
                            q.drain(..).collect()
                        };
                        if !drained.is_empty() {
                            let mut ec = EventContext::new(cx);
                            let target = ec.focused();
                            for entry in drained {
                                match entry {
                                    KeyInject::Char(c) => {
                                        ec.emit_to(target, TextEvent::InsertText(c.to_string()));
                                    }
                                    KeyInject::ControlKey(code, key) => {
                                        ec.emit_to(target, WindowEvent::KeyDown(code, Some(key)));
                                    }
                                }
                            }
                        }
                    }
                });

                if theming == ViziaTheming::None {
                    application = application.ignore_default_theme();
                }

                application
            }
        };

        let vizia_window = if let Some(parent) = parent {
            let application = make_application();
            let handle = application.open_parented(&Rwh05Parent(parent));
            ViziaWindow {
                inner: Arc::new(Mutex::new(Some(handle))),
                open_parented_fn: Arc::new(Mutex::new(None)),
                open_blocking_fn: Arc::new(Mutex::new(None)),
            }
        } else if wait_for_parent {
            let open_fn = Box::new(move |parent: &Rwh05Parent| {
                let application = make_application();
                application.open_parented(parent)
            });
            ViziaWindow {
                inner: Arc::new(Mutex::new(None)),
                open_parented_fn: Arc::new(Mutex::new(Some(open_fn))),
                open_blocking_fn: Arc::new(Mutex::new(None)),
            }
        } else {
            let standalone_fn = Box::new(move || {
                let application = make_application();
                let _ = application.run();
            });
            ViziaWindow {
                inner: Arc::new(Mutex::new(None)),
                open_parented_fn: Arc::new(Mutex::new(None)),
                open_blocking_fn: Arc::new(Mutex::new(Some(standalone_fn))),
            }
        };

        self.vizia_state.open.store(true, Ordering::Release);

        let handle = ViziaEditorHandle {
            vizia_state: self.vizia_state.clone(),
            window: vizia_window.clone(),
            param_registry: self.param_registry.clone(),
            emit_parameters_changed_event: self.emit_parameters_changed_event.clone(),
            key_inject: self.key_inject.clone(),
        };

        Ok(SpawnedEditor {
            handle,
            window: vizia_window,
        })
    }

    fn size(&self) -> NativeSize<u32> {
        let (width, height) = self.vizia_state.scaled_logical_size();
        NativeSize::new(width, height)
    }

    fn resize_hint(&self) -> ResizeHint {
        let (width, height) = self.vizia_state.inner_logical_size();
        ResizeHint {
            can_resize: true,
            can_resize_horizontally: true,
            can_resize_vertically: true,
            preserve_aspect_ratio: true,
            aspect_ratio_width: width.max(1),
            aspect_ratio_height: height.max(1),
            size_constraints: SizeConstraints::default(),
        }
    }
}

pub struct ViziaEditorHandle {
    pub(crate) vizia_state: Arc<ViziaState>,
    pub(crate) window: ViziaWindow,
    pub(crate) param_registry: ParamRegistry,
    pub(crate) emit_parameters_changed_event: Arc<AtomicBool>,
    pub(crate) key_inject: Arc<KeyInjectState>,
}

unsafe impl Send for ViziaEditorHandle {}

impl EditorHandle for ViziaEditorHandle {
    type Window = ViziaWindow;
    type Error = ViziaEditorError;

    fn run_until_closed(window: Self::Window) -> Result<(), Self::Error> {
        let op = window
            .open_blocking_fn
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(f) = op {
            f();
        }
        Ok(())
    }

    fn set_parent(
        &self,
        parent: ParentWindowHandle,
        window: &Self::Window,
    ) -> Result<(), Self::Error> {
        let op = window
            .open_parented_fn
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(f) = op {
            let handle = f(&Rwh05Parent(parent));
            *window
                .inner
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(handle);
        }
        Ok(())
    }

    fn show(&self, _window: &Self::Window) -> Result<(), Self::Error> {
        Ok(())
    }

    fn hide(&self, _window: &Self::Window) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_size(
        &self,
        new_size: NativeSize<u32>,
        _window: &Self::Window,
    ) -> Result<(), Self::Error> {
        let (base_width, base_height) = self.vizia_state.inner_logical_size();
        if new_size.width == 0 || new_size.height == 0 || base_width == 0 || base_height == 0 {
            return Ok(());
        }

        // The host size is already in the same coordinate space the idle
        // loop derives from once DPI is accounted for, so derive with a
        // neutral current scale and unit DPI: the requested size implies
        // the scale directly. Sharing `derive_scale` gives this path the
        // same 0.2–5.0 clamp and epsilon as the free-resize path, so a
        // transient tiny host size can no longer persist a degenerate
        // scale. (`set_user_scale_factor` would clamp again; the point
        // is the single derivation, not the second clamp.)
        let scale = derive_scale(
            1.0,
            new_size.width as f64,
            new_size.height as f64,
            1.0,
            base_width as f64,
            base_height as f64,
        );
        self.vizia_state.set_user_scale_factor(scale);
        Ok(())
    }

    fn adjust_size(
        &self,
        new_size: NativeSize<u32>,
        _window: &Self::Window,
    ) -> Option<NativeSize<u32>> {
        let (base_width, base_height) = self.vizia_state.inner_logical_size();
        if base_width == 0 || base_height == 0 {
            return Some(new_size);
        }
        let scale = (new_size.width as f64 / base_width as f64)
            .min(new_size.height as f64 / base_height as f64);
        Some(NativeSize::new(
            (base_width as f64 * scale).round().max(1.0) as u32,
            (base_height as f64 * scale).round().max(1.0) as u32,
        ))
    }

    fn host_main_thread_callback(&self, _window: &Self::Window) {}

    fn on_virtual_key_from_host(
        &self,
        key_code: VirtualKeyCode,
        is_down: bool,
        modifiers: Modifiers,
    ) -> bool {
        if !self.key_inject.text_focused.load(Ordering::Acquire) {
            return false;
        }

        if !modifiers.is_empty() {
            return false;
        }

        let inject = match key_code {
            VirtualKeyCode::Space => Some(KeyInject::Char(' ')),
            VirtualKeyCode::Numpad0 => Some(KeyInject::Char('0')),
            VirtualKeyCode::Numpad1 => Some(KeyInject::Char('1')),
            VirtualKeyCode::Numpad2 => Some(KeyInject::Char('2')),
            VirtualKeyCode::Numpad3 => Some(KeyInject::Char('3')),
            VirtualKeyCode::Numpad4 => Some(KeyInject::Char('4')),
            VirtualKeyCode::Numpad5 => Some(KeyInject::Char('5')),
            VirtualKeyCode::Numpad6 => Some(KeyInject::Char('6')),
            VirtualKeyCode::Numpad7 => Some(KeyInject::Char('7')),
            VirtualKeyCode::Numpad8 => Some(KeyInject::Char('8')),
            VirtualKeyCode::Numpad9 => Some(KeyInject::Char('9')),
            VirtualKeyCode::NumpadMultiply => Some(KeyInject::Char('*')),
            VirtualKeyCode::NumpadAdd => Some(KeyInject::Char('+')),
            VirtualKeyCode::NumpadSeparator => Some(KeyInject::Char(',')),
            VirtualKeyCode::NumpadSubtract => Some(KeyInject::Char('-')),
            VirtualKeyCode::NumpadDecimal => Some(KeyInject::Char('.')),
            VirtualKeyCode::NumpadDivide => Some(KeyInject::Char('/')),
            VirtualKeyCode::Equals => Some(KeyInject::Char('=')),

            VirtualKeyCode::Backspace => {
                Some(KeyInject::ControlKey(Code::Backspace, Key::Backspace))
            }
            VirtualKeyCode::Tab => Some(KeyInject::ControlKey(Code::Tab, Key::Tab)),
            VirtualKeyCode::Return => Some(KeyInject::ControlKey(Code::Enter, Key::Enter)),
            VirtualKeyCode::NumpadEnter => {
                Some(KeyInject::ControlKey(Code::NumpadEnter, Key::Enter))
            }
            VirtualKeyCode::Pause => Some(KeyInject::ControlKey(Code::Pause, Key::Pause)),
            VirtualKeyCode::Escape => Some(KeyInject::ControlKey(Code::Escape, Key::Escape)),
            VirtualKeyCode::End => Some(KeyInject::ControlKey(Code::End, Key::End)),
            VirtualKeyCode::Home => Some(KeyInject::ControlKey(Code::Home, Key::Home)),
            VirtualKeyCode::ArrowLeft => {
                Some(KeyInject::ControlKey(Code::ArrowLeft, Key::ArrowLeft))
            }
            VirtualKeyCode::ArrowUp => Some(KeyInject::ControlKey(Code::ArrowUp, Key::ArrowUp)),
            VirtualKeyCode::ArrowRight => {
                Some(KeyInject::ControlKey(Code::ArrowRight, Key::ArrowRight))
            }
            VirtualKeyCode::ArrowDown => {
                Some(KeyInject::ControlKey(Code::ArrowDown, Key::ArrowDown))
            }
            VirtualKeyCode::PageUp => Some(KeyInject::ControlKey(Code::PageUp, Key::PageUp)),
            VirtualKeyCode::PageDown => Some(KeyInject::ControlKey(Code::PageDown, Key::PageDown)),
            VirtualKeyCode::Insert => Some(KeyInject::ControlKey(Code::Insert, Key::Insert)),
            VirtualKeyCode::Delete => Some(KeyInject::ControlKey(Code::Delete, Key::Delete)),
            VirtualKeyCode::F1 => Some(KeyInject::ControlKey(Code::F1, Key::F1)),
            VirtualKeyCode::F2 => Some(KeyInject::ControlKey(Code::F2, Key::F2)),
            VirtualKeyCode::F3 => Some(KeyInject::ControlKey(Code::F3, Key::F3)),
            VirtualKeyCode::F4 => Some(KeyInject::ControlKey(Code::F4, Key::F4)),
            VirtualKeyCode::F5 => Some(KeyInject::ControlKey(Code::F5, Key::F5)),
            VirtualKeyCode::F6 => Some(KeyInject::ControlKey(Code::F6, Key::F6)),
            VirtualKeyCode::F7 => Some(KeyInject::ControlKey(Code::F7, Key::F7)),
            VirtualKeyCode::F8 => Some(KeyInject::ControlKey(Code::F8, Key::F8)),
            VirtualKeyCode::F9 => Some(KeyInject::ControlKey(Code::F9, Key::F9)),
            VirtualKeyCode::F10 => Some(KeyInject::ControlKey(Code::F10, Key::F10)),
            VirtualKeyCode::F11 => Some(KeyInject::ControlKey(Code::F11, Key::F11)),
            VirtualKeyCode::F12 => Some(KeyInject::ControlKey(Code::F12, Key::F12)),

            _ => None,
        };

        let Some(entry) = inject else {
            return false;
        };

        if is_down {
            self.key_inject
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push_back(entry);
        }
        true
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {
        self.param_registry.flush_all();
        self.emit_parameters_changed_event
            .store(true, Ordering::Relaxed);
    }

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {
        self.param_registry.flush_all();
        self.emit_parameters_changed_event
            .store(true, Ordering::Relaxed);
    }
}

impl Drop for ViziaEditorHandle {
    fn drop(&mut self) {
        self.vizia_state.open.store(false, Ordering::Release);
        if let Some(mut handle) = self
            .window
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            handle.close();
        }
        self.param_registry.clear_signals();
    }
}

#[cfg(test)]
mod tests {
    use super::derive_scale;

    const BASE_W: f64 = 1080.0;
    const BASE_H: f64 = 825.0;

    #[test]
    fn consistent_state_derives_current_scale() {
        // Bounds equal base × DPI (DPI already includes the user scale):
        // ratios are 1.0, so the scale is unchanged at any DPI.
        assert_eq!(
            derive_scale(1.5, BASE_W, BASE_H, 1.0, BASE_W, BASE_H),
            1.5
        );
        assert_eq!(
            derive_scale(1.5, BASE_W * 2.0, BASE_H * 2.0, 2.0, BASE_W, BASE_H),
            1.5
        );
        assert_eq!(
            derive_scale(0.2, BASE_W * 0.2, BASE_H * 0.2, 1.0, BASE_W, BASE_H),
            0.2
        );
    }

    #[test]
    fn uniform_drag_scales_proportionally() {
        assert_eq!(
            derive_scale(1.0, BASE_W * 1.5, BASE_H * 1.5, 1.0, BASE_W, BASE_H),
            1.5
        );
        assert_eq!(
            derive_scale(2.0, BASE_W * 0.5, BASE_H * 0.5, 1.0, BASE_W, BASE_H),
            1.0
        );
    }

    #[test]
    fn single_axis_drags_derive_current_scale() {
        // The min() means a width-only or height-only drag implies no
        // scale change (unless the host honors preserve_aspect_ratio).
        assert_eq!(
            derive_scale(1.0, BASE_W * 1.5, BASE_H, 1.0, BASE_W, BASE_H),
            1.0
        );
        assert_eq!(
            derive_scale(1.0, BASE_W, BASE_H * 1.5, 1.0, BASE_W, BASE_H),
            1.0
        );
    }

    #[test]
    fn degenerate_inputs_derive_current_scale() {
        assert_eq!(derive_scale(1.25, BASE_W, BASE_H, 0.0, BASE_W, BASE_H), 1.25);
        assert_eq!(
            derive_scale(1.25, BASE_W, BASE_H, -1.0, BASE_W, BASE_H),
            1.25
        );
        assert_eq!(derive_scale(1.25, 0.0, BASE_H, 1.0, BASE_W, BASE_H), 1.25);
        assert_eq!(derive_scale(1.25, BASE_W, 0.0, 1.0, BASE_W, BASE_H), 1.25);
        assert_eq!(derive_scale(1.25, BASE_W, BASE_H, 1.0, 0.0, BASE_H), 1.25);
        assert_eq!(
            derive_scale(1.25, f64::INFINITY, BASE_H, 1.0, BASE_W, BASE_H),
            1.25
        );
        assert_eq!(derive_scale(1.25, f64::NAN, BASE_H, 1.0, BASE_W, BASE_H), 1.25);
    }

    #[test]
    fn epsilon_absorbs_pixel_rounding() {
        // A 0.1% size wobble (integer-pixel rounding) is within epsilon.
        assert_eq!(
            derive_scale(1.0, BASE_W * 1.001, BASE_H * 1.001, 1.0, BASE_W, BASE_H),
            1.0
        );
        // Just past the epsilon, the change applies.
        let scaled = derive_scale(1.0, BASE_W * 1.003, BASE_H * 1.003, 1.0, BASE_W, BASE_H);
        assert!((scaled - 1.003).abs() < 1e-9, "scaled = {scaled}");
    }

    #[test]
    fn clamp_rails_bound_degenerate_sizes() {
        assert_eq!(
            derive_scale(1.0, BASE_W * 100.0, BASE_H * 100.0, 1.0, BASE_W, BASE_H),
            5.0
        );
        assert_eq!(
            derive_scale(1.0, 1.0, 1.0, 1.0, BASE_W, BASE_H),
            0.2
        );
    }
}
