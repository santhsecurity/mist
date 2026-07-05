//! Runtime-generated tray icon for Mist.
//!
//! We render the icon with tiny-skia instead of shipping a PNG so the binary
//! stays self-contained and looks crisp on any DPI.

use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Generate a 64×64 RGBA icon for the system tray.
#[must_use]
pub fn app_icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let size = 64u32;
    let mut pixmap = Pixmap::new(size, size)?;

    // Dark rounded-square background.
    let r = 14.0;
    let bg = rounded_rect(0.0, 0.0, size as f32, size as f32, r);
    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(18, 18, 20, 255);
    bg_paint.anti_alias = true;
    pixmap.fill_path(&bg, &bg_paint, FillRule::Winding, Transform::identity(), None);

    // White "M" stroke.
    let mut pb = PathBuilder::new();
    let margin = 18.0;
    let top = 16.0;
    let bottom = 48.0;
    let mid_x = size as f32 / 2.0;
    pb.move_to(margin, bottom);
    pb.line_to(margin, top);
    pb.line_to(mid_x, 32.0);
    pb.line_to(size as f32 - margin, top);
    pb.line_to(size as f32 - margin, bottom);
    let path = pb.finish()?;

    let stroke = Stroke {
        width: 5.0,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Stroke::default()
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);

    Some((unpremultiply_rgba(pixmap.data()), size, size))
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let r = r.min(h / 2.0).min(w / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w, y, x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h, x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x, y + h, x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y, x, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}

fn unpremultiply_rgba(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks_exact(4) {
        let a = chunk[3];
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let r = ((u16::from(chunk[0]) * 255) / u16::from(a)) as u8;
            let g = ((u16::from(chunk[1]) * 255) / u16::from(a)) as u8;
            let b = ((u16::from(chunk[2]) * 255) / u16::from(a)) as u8;
            out.extend_from_slice(&[r, g, b, a]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_icon_has_expected_dimensions() {
        let (rgba, width, height) = app_icon_rgba().expect("icon should render");
        assert_eq!(width, 64);
        assert_eq!(height, 64);
        assert_eq!(rgba.len(), (width * height * 4) as usize);
    }
}
