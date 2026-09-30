//! Small shared compatibility layer for custom Vizia/Skia widgets.

use vizia_plug::vizia::vg;

pub struct Color(vg::Color4f);

impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgbaf(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgbaf(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    pub fn rgbaf(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self(vg::Color4f::new(r, g, b, a))
    }
}

pub enum Align {
    Left,
    Center,
    Right,
}

pub enum Solidity {
    Hole,
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

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.builder
            .add_rect(vg::Rect::from_xywh(x, y, w, h), None, None);
    }

    pub fn rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32) {
        let rect = vg::Rect::from_xywh(x, y, w, h);
        self.builder
            .add_rrect(vg::RRect::new_rect_xy(&rect, radius, radius), None, None);
    }

    pub fn circle(&mut self, x: f32, y: f32, radius: f32) {
        self.builder.add_circle((x, y), radius, None);
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to((x, y));
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to((x, y));
    }

    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.builder.quad_to((cx, cy), (x, y));
    }

    pub fn close(&mut self) {
        self.builder.close();
    }

    pub fn arc(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        start: f32,
        end: f32,
        _solidity: Solidity,
    ) {
        let rect = vg::Rect::from_xywh(cx - radius, cy - radius, radius * 2.0, radius * 2.0);
        self.builder
            .add_arc(rect, start.to_degrees(), (end - start).to_degrees());
    }

    fn snapshot(&self) -> vg::Path {
        self.builder.snapshot()
    }
}

pub struct Paint {
    inner: vg::Paint,
    font_size: f32,
    align: Align,
}

impl Paint {
    pub fn color(color: Color) -> Self {
        let mut inner = vg::Paint::default();
        inner.set_anti_alias(true);
        inner.set_color4f(&color.0, None);
        Self {
            inner,
            font_size: 12.0,
            align: Align::Left,
        }
    }

    pub fn with_line_width(mut self, width: f32) -> Self {
        self.set_line_width(width);
        self
    }

    pub fn set_line_width(&mut self, width: f32) {
        self.inner.set_style(vg::PaintStyle::Stroke);
        self.inner.set_stroke_width(width);
    }

    pub fn set_font_size(&mut self, size: f32) {
        self.font_size = size;
    }

    pub fn set_text_align(&mut self, align: Align) {
        self.align = align;
    }
}

pub trait CanvasExt {
    fn fill_path(&self, path: &Path, paint: &Paint);
    fn stroke_path(&self, path: &Path, paint: &Paint);
    fn fill_text(&self, x: f32, y: f32, text: impl AsRef<str>, paint: &Paint);
}

impl CanvasExt for vg::Canvas {
    fn fill_path(&self, path: &Path, paint: &Paint) {
        let mut paint = paint.inner.clone();
        paint.set_style(vg::PaintStyle::Fill);
        self.draw_path(&path.snapshot(), &paint);
    }

    fn stroke_path(&self, path: &Path, paint: &Paint) {
        self.draw_path(&path.snapshot(), &paint.inner);
    }

    fn fill_text(&self, x: f32, y: f32, text: impl AsRef<str>, paint: &Paint) {
        let mut font = vg::Font::default();
        font.set_size(paint.font_size);
        let align = match paint.align {
            Align::Left => vg::utils::text_utils::Align::Left,
            Align::Center => vg::utils::text_utils::Align::Center,
            Align::Right => vg::utils::text_utils::Align::Right,
        };
        self.draw_text_align(text.as_ref(), (x, y), &font, &paint.inner, align);
    }
}
