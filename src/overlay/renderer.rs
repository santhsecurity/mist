use std::time::Instant;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Visual state of the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayState {
    Listening,
    Processing,
    Done,
    Error,
}

/// Minimal monochrome bar renderer for the Mist overlay.
///
/// No waveforms, no neon, no gradients - just a sleek black pill with white
/// typography. The state is communicated by the text itself.
pub struct Renderer {
    width: u32,
    height: u32,
    state: OverlayState,
    text: Option<String>,
    font: Option<fontdue::Font>,
    start: Instant,
}

impl Renderer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            state: OverlayState::Listening,
            text: None,
            font: load_font(),
            start: Instant::now(),
        }
    }

    pub fn with_font(mut self, font: Option<fontdue::Font>) -> Self {
        self.font = font;
        self
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn take_font(&mut self) -> Option<fontdue::Font> {
        self.font.take()
    }

    pub fn set_state(&mut self, state: OverlayState) {
        self.state = state;
    }

    /// Kept for API compatibility. The monochrome bar does not render a
    /// waveform, so incoming audio samples are ignored.
    pub fn set_waveform_samples(&mut self, _samples: &[f32]) {}

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = Some(text.into());
    }

    pub fn clear_text(&mut self) {
        self.text = None;
    }

    pub fn reset_time(&mut self) {
        self.start = Instant::now();
    }

    /// Render one frame and return the unpremultiplied RGBA pixel buffer.
    pub fn render(&mut self) -> Vec<u8> {
        let mut pixmap = Pixmap::new(self.width, self.height)
            .unwrap_or_else(|| Pixmap::new(1, 1).unwrap());

        let w = self.width as f32;
        let h = self.height as f32;
        let r = h / 2.0;

        let capsule = rounded_capsule(0.0, 0.0, w, h, r);

        // Near-black fill.
        let mut fill_paint = Paint::default();
        fill_paint.set_color_rgba8(10, 10, 10, 245);
        fill_paint.anti_alias = true;
        pixmap.fill_path(&capsule, &fill_paint, FillRule::Winding, Transform::identity(), None);

        // Subtle white hairline border.
        let mut border_paint = Paint::default();
        border_paint.set_color_rgba8(255, 255, 255, 22);
        border_paint.anti_alias = true;
        let border_stroke = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        pixmap.stroke_path(&capsule, &border_paint, &border_stroke, Transform::identity(), None);

        // Text is the entire UI. Center it in the bar.
        let text = self.text.as_deref().unwrap_or("");
        if let Some(font) = &self.font {
            let px = 14.0;
            let text_width = measure_text_width(font, text, px);
            let text_x = ((w - text_width) / 2.0).max(12.0);
            let max_text_w = (w - text_x - 12.0) as i32;
            draw_text_rgba(
                pixmap.data_mut(),
                self.width,
                self.height,
                text_x as i32,
                (h / 2.0) as i32,
                text,
                px,
                max_text_w,
                font,
            );
        }

        pixmap_to_rgba(&pixmap)
    }
}

fn rounded_capsule(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let r = r.min(h / 2.0).min(w / 2.0);
    rounded_rect_path(x, y, w, h, r)
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
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

fn pixmap_to_rgba(pixmap: &Pixmap) -> Vec<u8> {
    let data = pixmap.data();
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks_exact(4) {
        let a = chunk[3];
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let r = ((chunk[0] as u16 * 255) / a as u16) as u8;
            let g = ((chunk[1] as u16 * 255) / a as u16) as u8;
            let b = ((chunk[2] as u16 * 255) / a as u16) as u8;
            out.extend_from_slice(&[r, g, b, a]);
        }
    }
    out
}

fn measure_text_width(font: &fontdue::Font, text: &str, px: f32) -> f32 {
    let mut width = 0.0;
    let mut prev: Option<char> = None;
    for ch in text.chars() {
        if let Some(k) = prev.and_then(|p| font.horizontal_kern(p, ch, px)) {
            width += k;
        }
        let (metrics, _) = font.rasterize(ch, px);
        width += metrics.advance_width;
        prev = Some(ch);
    }
    width
}

#[allow(clippy::too_many_arguments)]
fn draw_text_rgba(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    _y: i32,
    text: &str,
    px: f32,
    max_width: i32,
    font: &fontdue::Font,
) {
    // Render text at 2× resolution then downsample for cleaner strokes.
    let scale = 2.0;
    let px2 = px * scale;
    let width2 = width as usize * scale as usize;
    let height2 = height as usize * scale as usize;

    let ascent = font
        .horizontal_line_metrics(px2)
        .map(|m| m.ascent)
        .unwrap_or(px2 * 0.75);
    // Center the cap height in the capsule.
    let baseline2 = ((height as f32 * scale + ascent) / 2.0 - 4.0) as i32;

    let mut cursor_x = x as f32;
    let mut prev: Option<char> = None;
    let ellipsis = '…';
    let mut chars_to_draw: Vec<char> = Vec::new();

    for ch in text.chars() {
        let (metrics, _) = font.rasterize(ch, px);
        let advance = metrics.advance_width;
        if (cursor_x + advance) as i32 > x + max_width {
            if chars_to_draw.len() > 1 {
                chars_to_draw.pop();
                let (em, _) = font.rasterize(ellipsis, px);
                if (cursor_x - advance + em.advance_width) as i32 <= x + max_width {
                    chars_to_draw.push(ellipsis);
                }
            }
            break;
        }
        cursor_x += advance;
        chars_to_draw.push(ch);
    }

    let mut mask = vec![0u8; width2 * height2];
    cursor_x = x as f32 * scale;
    for ch in &chars_to_draw {
        if let Some(k) = prev.and_then(|p| font.horizontal_kern(p, *ch, px2)) {
            cursor_x += k;
        }
        let (metrics, bitmap) = font.rasterize(*ch, px2);
        let gx = cursor_x as i32 + metrics.xmin;
        let gy = baseline2 + metrics.ymin;
        blit_glyph_mask(
            &mut mask,
            width2 as u32,
            height2 as u32,
            gx,
            gy,
            &bitmap,
            metrics.width,
            metrics.height,
            0.95,
        );
        cursor_x += metrics.advance_width;
        prev = Some(*ch);
    }

    for ty in 0..height {
        for tx in 0..width {
            let sx = tx as usize * scale as usize;
            let sy = ty as usize * scale as usize;
            let a = (mask[sy * width2 + sx] as u32
                + mask[sy * width2 + sx + 1] as u32
                + mask[(sy + 1) * width2 + sx] as u32
                + mask[(sy + 1) * width2 + sx + 1] as u32)
                / 4;
            if a == 0 {
                continue;
            }
            let alpha = a as f32 / 255.0;
            let idx = ((ty * width + tx) * 4) as usize;
            let inv = 1.0 - alpha;
            buffer[idx] = (255.0 * alpha + buffer[idx] as f32 * inv) as u8;
            buffer[idx + 1] = (255.0 * alpha + buffer[idx + 1] as f32 * inv) as u8;
            buffer[idx + 2] = (255.0 * alpha + buffer[idx + 2] as f32 * inv) as u8;
            let old_a = buffer[idx + 3] as f32 / 255.0;
            let new_a = alpha + old_a * inv;
            buffer[idx + 3] = (new_a * 255.0) as u8;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_glyph_mask(
    mask: &mut [u8],
    width: u32,
    height: u32,
    gx: i32,
    gy: i32,
    bitmap: &[u8],
    w: usize,
    h: usize,
    alpha_mul: f32,
) {
    for row in 0..h {
        for col in 0..w {
            let px = gx + col as i32;
            let py = gy + row as i32;
            if px < 0 || px >= width as i32 || py < 0 || py >= height as i32 {
                continue;
            }
            let alpha = (bitmap[row * w + col] as f32 * alpha_mul) as u8;
            let idx = (py as u32 * width + px as u32) as usize;
            if alpha > mask[idx] {
                mask[idx] = alpha;
            }
        }
    }
}

fn load_font() -> Option<fontdue::Font> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    let families: Vec<fontdb::Family> = vec![
        fontdb::Family::Name("Inter"),
        fontdb::Family::Name("SF Pro Text"),
        fontdb::Family::Name("SF Pro"),
        fontdb::Family::Name("Segoe UI"),
        fontdb::Family::Name("Helvetica Neue"),
        fontdb::Family::Name("Helvetica"),
        fontdb::Family::Name("Arial"),
        fontdb::Family::SansSerif,
    ];

    for family in &families {
        let id = db.query(&fontdb::Query {
            families: std::slice::from_ref(family),
            weight: fontdb::Weight::NORMAL,
            stretch: fontdb::Stretch::Normal,
            style: fontdb::Style::Normal,
        });
        if let Some(id) = id {
            if let Some(font) = db.with_face_data(id, |data, _index| {
                #[allow(clippy::unnecessary_to_owned)]
                fontdue::Font::from_bytes(
                    data.to_vec(),
                    fontdue::FontSettings {
                        collection_index: 0,
                        scale: 30.0,
                        load_substitutions: true,
                    },
                )
                .ok()
            }) {
                return font;
            }
        }
    }
    None
}

/// Kept for API compatibility with the audio preview path.
pub fn waveform_from_samples(samples: &[f32], target_len: usize) -> Vec<f32> {
    if samples.is_empty() || target_len == 0 {
        return vec![0.0; target_len];
    }
    let window = (16000.0 * 0.22) as usize;
    let start = samples.len().saturating_sub(window);
    let recent = &samples[start..];
    let mut out = Vec::with_capacity(target_len);
    let step = recent.len() as f32 / target_len as f32;
    for i in 0..target_len {
        let idx = ((i as f32 * step) as usize).min(recent.len().saturating_sub(1));
        out.push(recent[idx]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_from_samples_empty() {
        assert!(waveform_from_samples(&[], 8).iter().all(|&v| v == 0.0));
    }

    #[test]
    fn waveform_from_samples_length() {
        let samples: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        assert_eq!(waveform_from_samples(&samples, 16).len(), 16);
    }
}
