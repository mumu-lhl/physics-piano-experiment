//! A draggable velocity-curve editor: five points at fixed input abscissae
//! (0, 0.25, 0.5, 0.75, 1), with the three interior points draggable
//! vertically. The endpoints are locked at 0 and 1, so the curve always maps
//! silence to silence and full velocity to full velocity.
//!
//! Each interior point is bound to a plugin parameter (Linear 0..1), and
//! drags are bracketed by begin/end parameter gestures exactly like the
//! knob wrapper, so the host records one undo step per drag and the
//! editor's gesture history coalesces the moves. Double-click or
//! right-click on a handle resets that point to its parameter default
//! (the linear value).

use crate::skia_compat as vg;
use crate::skia_compat::CanvasExt;
use nice_plug::prelude::Param;
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::param_base::ParamWidgetBase;

/// Fixed input positions of the five curve points.
const POINT_X: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
/// Horizontal inset so the endpoint handles stay inside the widget.
const PAD_X: f32 = 10.0;
/// Vertical inset so handles at 0 and 1 stay inside the widget.
const PAD_Y: f32 = 6.0;
/// Grab radius around an interior handle.
const GRAB_R: f32 = 11.0;

pub struct VelocityCurveEditorWidget {
    /// One gesture base per interior point parameter (25%, 50%, 75%).
    bases: [ParamWidgetBase; 3],
    /// Interior point currently being dragged (0..3, indexing `bases`).
    dragging: Option<usize>,
}

impl VelocityCurveEditorWidget {
    pub fn new<'c, 'p, P>(
        cx: &'c mut Context,
        point_25: &'p P,
        point_50: &'p P,
        point_75: &'p P,
    ) -> Handle<'c, Self>
    where
        'p: 'c,
        P: Param + 'static,
    {
        Self {
            bases: [
                ParamWidgetBase::new(cx, point_25),
                ParamWidgetBase::new(cx, point_50),
                ParamWidgetBase::new(cx, point_75),
            ],
            dragging: None,
        }
        .build(cx, |_| {})
    }

    /// All five point values; endpoints are locked, interior points come
    /// from their parameters (Linear 0..1, so normalized == plain).
    fn point_values(&self) -> [f32; 5] {
        [
            0.0,
            self.bases[0].unmodulated_normalized_value(),
            self.bases[1].unmodulated_normalized_value(),
            self.bases[2].unmodulated_normalized_value(),
            1.0,
        ]
    }

    fn point_pos(&self, bounds: &BoundingBox, i: usize, values: &[f32; 5]) -> (f32, f32) {
        let plot_w = bounds.w - 2.0 * PAD_X;
        let plot_h = bounds.h - 2.0 * PAD_Y;
        (
            bounds.x + PAD_X + POINT_X[i] * plot_w,
            bounds.y + PAD_Y + (1.0 - values[i]) * plot_h,
        )
    }

    /// Interior point (1..5 in point indexing) whose handle is near (x, y).
    fn hit_point(&self, bounds: &BoundingBox, x: f32, y: f32) -> Option<usize> {
        let values = self.point_values();
        (1..4).find(|&i| {
            let (px, py) = self.point_pos(bounds, i, &values);
            (x - px).powi(2) + (y - py).powi(2) <= GRAB_R * GRAB_R
        })
    }

    fn reset_point(&self, cx: &mut EventContext, index: usize) {
        let base = &self.bases[index];
        base.begin_set_parameter(cx);
        base.set_normalized_value(cx, base.default_normalized_value());
        base.end_set_parameter(cx);
    }

    /// Ends an in-flight drag gesture, if any (used when a reset click
    /// lands mid-drag so the gesture is never left un-ended).
    fn end_drag(&mut self, cx: &mut EventContext) {
        if let Some(i) = self.dragging.take() {
            cx.release();
            self.bases[i].end_set_parameter(cx);
        }
    }
}

impl View for VelocityCurveEditorWidget {
    fn element(&self) -> Option<&'static str> {
        Some("velocity-curve-editor")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let bounds = cx.bounds();
                let (mx, my) = (cx.mouse().cursor_x, cx.mouse().cursor_y);
                if let Some(i) = self.hit_point(&bounds, mx, my) {
                    self.dragging = Some(i - 1);
                    cx.capture();
                    cx.focus();
                    self.bases[i - 1].begin_set_parameter(cx);
                    meta.consume();
                }
            }
            WindowEvent::MouseMove(_, y) => {
                if let Some(i) = self.dragging {
                    let bounds = cx.bounds();
                    let plot_h = bounds.h - 2.0 * PAD_Y;
                    let value = if plot_h > 0.0 {
                        ((bounds.y + bounds.h - PAD_Y - *y) / plot_h).clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    self.bases[i].set_normalized_value(cx, value);
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.dragging.is_some() {
                    self.end_drag(cx);
                    meta.consume();
                }
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                let bounds = cx.bounds();
                let (mx, my) = (cx.mouse().cursor_x, cx.mouse().cursor_y);
                if let Some(i) = self.hit_point(&bounds, mx, my) {
                    self.end_drag(cx);
                    self.reset_point(cx, i - 1);
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::MouseDown(MouseButton::Right) => {
                let bounds = cx.bounds();
                let (mx, my) = (cx.mouse().cursor_x, cx.mouse().cursor_y);
                if let Some(i) = self.hit_point(&bounds, mx, my) {
                    self.end_drag(cx);
                    self.reset_point(cx, i - 1);
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        if bounds.w < 20.0 || bounds.h < 10.0 {
            return;
        }
        let values = self.point_values();
        let plot_w = bounds.w - 2.0 * PAD_X;
        let plot_h = bounds.h - 2.0 * PAD_Y;
        let px = |i: usize| bounds.x + PAD_X + POINT_X[i] * plot_w;
        let py = |v: f32| bounds.y + PAD_Y + (1.0 - v) * plot_h;

        // Background track.
        let mut bg_path = vg::Path::new();
        bg_path.rounded_rect(bounds.x, bounds.y, bounds.w, bounds.h, 3.0);
        canvas.fill_path(&bg_path, &vg::Paint::color(vg::Color::rgb(14, 16, 22)));
        canvas.stroke_path(
            &bg_path,
            &vg::Paint::color(vg::Color::rgb(45, 50, 65)).with_line_width(0.75),
        );

        // Horizontal grid lines at 0.25 / 0.5 / 0.75.
        let grid_paint = vg::Paint::color(vg::Color::rgb(35, 38, 52)).with_line_width(0.75);
        for gv in [0.25f32, 0.5, 0.75] {
            let mut line = vg::Path::new();
            line.move_to(bounds.x + PAD_X, py(gv));
            line.line_to(bounds.x + bounds.w - PAD_X, py(gv));
            canvas.stroke_path(&line, &grid_paint);
        }

        // Linear reference diagonal.
        let mut diag = vg::Path::new();
        diag.move_to(bounds.x + PAD_X, py(0.0));
        diag.line_to(bounds.x + bounds.w - PAD_X, py(1.0));
        canvas.stroke_path(
            &diag,
            &vg::Paint::color(vg::Color::rgb(80, 85, 105)).with_line_width(0.75),
        );

        // Curve polyline.
        let gold = vg::Paint::color(vg::Color::rgb(229, 184, 66)).with_line_width(1.5);
        let mut curve = vg::Path::new();
        curve.move_to(px(0), py(values[0]));
        for i in 1..5 {
            curve.line_to(px(i), py(values[i]));
        }
        canvas.stroke_path(&curve, &gold);

        // Handles: dim fixed endpoints, gold draggable interior points
        // (enlarged while dragged).
        for i in 0..5 {
            let (x, y) = (px(i), py(values[i]));
            if i == 0 || i == 4 {
                let mut handle = vg::Path::new();
                handle.circle(x, y, 4.0);
                canvas.fill_path(&handle, &vg::Paint::color(vg::Color::rgb(120, 125, 140)));
            } else {
                let r = if self.dragging == Some(i - 1) {
                    8.0
                } else {
                    6.0
                };
                let mut handle = vg::Path::new();
                handle.circle(x, y, r);
                canvas.fill_path(&handle, &gold);
            }
        }
    }
}
