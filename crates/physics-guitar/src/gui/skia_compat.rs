//! Small compatibility layer for the custom fretboard migrated from femtovg to Skia.

use vizia_plug::vizia::vg;

pub struct Color(vg::Color4f);

impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(vg::Color4f::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            1.0,
        ))
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(vg::Color4f::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ))
    }
}

pub struct Path {
    builder: vg::PathBuilder,
}

impl Path {
    pub fn new() -> Self {
        Self {
            builder: vg::PathBuilder::new(),
        }
    }

    pub fn rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32) {
        let rect = vg::Rect::from_xywh(x, y, w, h);
        self.builder
            .add_rrect(vg::RRect::new_rect_xy(&rect, radius, radius), None, None);
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to((x, y));
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to((x, y));
    }

    pub fn circle(&mut self, x: f32, y: f32, radius: f32) {
        self.builder.add_circle((x, y), radius, None);
    }

    fn snapshot(&self) -> vg::Path {
        self.builder.snapshot()
    }
}

pub struct Paint(vg::Paint);

impl Paint {
    pub fn color(color: Color) -> Self {
        let mut paint = vg::Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(&color.0, None);
        Self(paint)
    }

    pub fn set_line_width(&mut self, width: f32) {
        self.0.set_style(vg::PaintStyle::Stroke);
        self.0.set_stroke_width(width);
    }
}

pub trait CanvasExt {
    fn fill_path(&self, path: &Path, paint: &Paint);
    fn stroke_path(&self, path: &Path, paint: &Paint);
}

impl CanvasExt for vg::Canvas {
    fn fill_path(&self, path: &Path, paint: &Paint) {
        let mut paint = paint.0.clone();
        paint.set_style(vg::PaintStyle::Fill);
        self.draw_path(&path.snapshot(), &paint);
    }

    fn stroke_path(&self, path: &Path, paint: &Paint) {
        self.draw_path(&path.snapshot(), &paint.0);
    }
}
