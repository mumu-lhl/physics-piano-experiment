//! The [`Editor`] trait implementation for Vizia editors.

use crossbeam::atomic::AtomicCell;
use nice_plug_core::context::gui::GuiContext;
use nice_plug_core::debug::*;
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
                        nice_error!("Failed to load stylesheet: {err:?}");
                        panic!();
                    }
                    widgets::register_theme(cx);
                    param_registry.clone().build(cx);
                    widgets::ParamModel {
                        context: gui_context.clone(),
                    }
                    .build(cx);

                    let current_inner_window_size =
                        EventContext::new(cx).cache.get_bounds(Entity::root());
                    widgets::WindowModel {
                        context: gui_context.clone(),
                        vizia_state: vizia_state.clone(),
                        last_inner_window_size: AtomicCell::new((
                            current_inner_window_size.width() as u32,
                            current_inner_window_size.height() as u32,
                        )),
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
        let op = window.open_blocking_fn.lock().unwrap().take();
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
        let op = window.open_parented_fn.lock().unwrap().take();
        if let Some(f) = op {
            let handle = f(&Rwh05Parent(parent));
            *window.inner.lock().unwrap() = Some(handle);
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

        let scale = (new_size.width as f64 / base_width as f64)
            .min(new_size.height as f64 / base_height as f64);
        if scale.is_finite() && scale > 0.0 {
            self.vizia_state.set_user_scale_factor(scale);
        }
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
        if let Some(mut handle) = self.window.inner.lock().unwrap().take() {
            handle.close();
        }
        self.param_registry.clear_signals();
    }
}
