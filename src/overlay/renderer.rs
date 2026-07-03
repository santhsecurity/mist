use std::time::Instant;
use tiny_skia::{Color, FillRule, GradientStop, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Point, SpreadMode, Stroke, Transform};

/// Visual state of the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayState {
    Listening,
    Processing,
    Done,
    Error,
}

/// Refined, minimal software renderer for the Mist overlay.
///
/// Design notes:
/// - Monochrome, low-contrast capsule so it melts into the desktop.
/// - Single accent color for the waveform (soft indigo or amber), no neon.
/// - Clean system typography, no faux-glow text halos.
/// - Anti-aliased vector shapes only.
pub struct Renderer {
    width: u32,
    height: u32,
    state: OverlayState,
    samples: Vec<f32>,
    smoothed: Vec<f32>,
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
            samples: vec![0.0; 200],
            smoothed: vec![0.0; 200],
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

    /// Replace the current waveform samples. The renderer smoothly
    /// interpolates toward these values each frame.
    pub fn set_waveform_samples(&mut self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        let target_len = self.samples.len();
        self.samples.clear();
        self.samples.reserve(target_len);
        let step = samples.len() as f32 / target_len as f32;
        for i in 0..target_len {
            let idx = ((i as f32 * step) as usize).min(samples.len() - 1);
            self.samples.push(samples[idx]);
        }
    }

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

        // Smooth the waveform samples.
        let attack = 0.25;
        let decay = 0.10;
        for i in 0..self.smoothed.len() {
            let target = self.samples.get(i).copied().unwrap_or(0.0);
            let delta = target - self.smoothed[i];
            self.smoothed[i] += delta * if delta > 0.0 { attack } else { decay };
        }

        let elapsed = self.start.elapsed().as_secs_f32();
        let w = self.width as f32;
        let h = self.height as f32;
        let r = h / 2.0;

        let capsule = rounded_capsule(0.0, 0.0, w, h, r);

        // Capsule fill.
        let mut fill_paint = Paint::default();
        fill_paint.set_color_rgba8(24, 24, 27, 232);
        fill_paint.anti_alias = true;
        pixmap.fill_path(&capsule, &fill_paint, FillRule::Winding, Transform::identity(), None);

        // Hairline border.
        let mut border_paint = Paint::default();
        border_paint.set_color_rgba8(255, 255, 255, 22);
        border_paint.anti_alias = true;
        let mut border_stroke = Stroke::default();
        border_stroke.width = 1.0;
        pixmap.stroke_path(&capsule, &border_paint, &border_stroke, Transform::identity(), None);

        // State dot.
        draw_state_dot(&mut pixmap, self.state, elapsed, 16.0, h / 2.0);

        // Decide layout: if we're showing final text, let it breathe on the
        // right side; otherwise the waveform owns that space.
        let text = self.text.as_deref().unwrap_or("");
        let has_text = !text.is_empty();
        let wave_x0 = 34.0;
        let mut wave_x1 = w - 16.0;
        let mut text_x = 34.0;

        if has_text && self.state == OverlayState::Done {
            // No waveform in the final "done" state; text spans the capsule.
            wave_x1 = wave_x0;
        } else if has_text {
            // Reserve up to 160px for text; waveform gets the rest.
            let reserved_text = (w - 44.0).min(160.0);
            wave_x1 = (w - 16.0 - reserved_text).max(wave_x0 + 60.0);
            text_x = wave_x1 + 10.0;
        }

        if wave_x1 > wave_x0 + 2.0 {
            draw_waveform(&mut pixmap, &self.smoothed, self.state, wave_x0, wave_x1, h);
        }

        if has_text {
            let max_text_w = (w - text_x - 14.0) as i32;
            draw_text_rgba(
                pixmap.data_mut(),
                self.width,
                self.height,
                text_x as i32,
                self.height as i32 / 2,
                text,
                17.0,
                max_text_w,
                &self.font,
            );
        }

        pixmap_to_rgba(&pixmap)
    }
}

fn draw_state_dot(pixmap: &mut Pixmap, state: OverlayState, elapsed: f32, cx: f32, cy: f32) {
    let (r, g, b) = match state {
        OverlayState::Listening => (239, 68, 68),   // red-500
        OverlayState::Processing => (245, 158, 11), // amber-500
        OverlayState::Done => (34, 197, 94),        // green-500
        OverlayState::Error => (239, 68, 68),
    };

    let base_alpha: u8 = if state == OverlayState::Listening {
        // Gentle pulse.
        let pulse = (elapsed * 2.5).sin() * 15.0 + 200.0;
        pulse as u8
    } else {
        220
    };

    let radius = 4.5;

    // Soft outer ring using a tiny radial gradient.
    let ring = tiny_skia::RadialGradient::new(
        Point::from_xy(cx, cy),
        0.0,
        Point::from_xy(cx, cy),
        radius + 3.0,
        vec![
            GradientStop::new(0.0, Color::from_rgba8(r, g, b, (base_alpha as f32 * 0.35) as u8)),
            GradientStop::new(1.0, Color::from_rgba8(r, g, b, 0)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();
    let ring_path = {
        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, radius + 3.0);
        pb.finish().unwrap()
    };
    let mut ring_paint = Paint::default();
    ring_paint.shader = ring;
    ring_paint.anti_alias = true;
    pixmap.fill_path(&ring_path, &ring_paint, FillRule::Winding, Transform::identity(), None);

    // Solid core.
    let core_path = {
        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, radius);
        pb.finish().unwrap()
    };
    let mut core_paint = Paint::default();
    core_paint.set_color_rgba8(r, g, b, base_alpha);
    core_paint.anti_alias = true;
    pixmap.fill_path(&core_path, &core_paint, FillRule::Winding, Transform::identity(), None);
}

fn draw_waveform(
    pixmap: &mut Pixmap,
    samples: &[f32],
    state: OverlayState,
    x0: f32,
    x1: f32,
    height: f32,
) {
    if samples.len() < 2 {
        return;
    }

    let center = height / 2.0;
    let max_amp = (height / 2.0) - 10.0;

    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    let gain = if rms > 0.001 {
        max_amp / (rms * 8.0).max(0.2)
    } else {
        max_amp * 2.0
    };

    let points: Vec<(f32, f32)> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let t = i as f32 / (samples.len() - 1) as f32;
            let x = x0 + t * (x1 - x0);
            let y = center - (s * gain).clamp(-max_amp, max_amp);
            (x, y)
        })
        .collect();

    let path = smooth_path(&points);

    let (r, g, b) = match state {
        OverlayState::Listening => (165, 180, 252), // indigo-300
        OverlayState::Processing => (251, 191, 36), // amber-300
        _ => (156, 163, 175),                       // gray-400
    };

    // Subtle shadow stroke for depth.
    let mut shadow_stroke = Stroke::default();
    shadow_stroke.width = 3.0;
    shadow_stroke.line_cap = LineCap::Round;
    shadow_stroke.line_join = LineJoin::Round;
    let mut shadow_paint = Paint::default();
    shadow_paint.set_color_rgba8(0, 0, 0, 50);
    shadow_paint.anti_alias = true;
    pixmap.stroke_path(&path, &shadow_paint, &shadow_stroke, Transform::identity(), None);

    // Accent stroke.
    let mut stroke = Stroke::default();
    stroke.width = 1.5;
    stroke.line_cap = LineCap::Round;
    stroke.line_join = LineJoin::Round;
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, 230);
    paint.anti_alias = true;
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

/// Build a smooth path through points using cubic Bézier interpolation.
fn smooth_path(points: &[(f32, f32)]) -> tiny_skia::Path {
    if points.len() < 2 {
        let mut pb = PathBuilder::new();
        if let Some(&(x, y)) = points.first() {
            pb.move_to(x, y);
        }
        return pb.finish().unwrap_or_else(|| {
            let mut pb = PathBuilder::new();
            pb.move_to(0.0, 0.0);
            pb.finish().unwrap()
        });
    }

    let mut pb = PathBuilder::new();
    pb.move_to(points[0].0, points[0].1);

    for i in 0..points.len() - 1 {
        let p0 = points[i];
        let p1 = points[i + 1];
        let dx = p1.0 - p0.0;
        let cp1x = p0.0 + dx * 0.5;
        let cp1y = p0.1;
        let cp2x = p1.0 - dx * 0.5;
        let cp2y = p1.1;
        pb.cubic_to(cp1x, cp1y, cp2x, cp2y, p1.0, p1.1);
    }
    pb.finish().unwrap()
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

fn draw_text_rgba(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    _y: i32,
    text: &str,
    px: f32,
    max_width: i32,
    font: &Option<fontdue::Font>,
) {
    let Some(font) = font else { return };

    // Render text at 2× resolution then downsample. fontdue produces the best
    // possible unhinted outlines, but at small sizes a 2× supersample mask
    // trades a tiny amount of CPU for noticeably cleaner strokes.
    let scale = 2.0;
    let px2 = px * scale;
    let width2 = width as usize * scale as usize;
    let height2 = height as usize * scale as usize;

    let (ascent, descent) = font
        .horizontal_line_metrics(px2)
        .map(|m| (m.ascent, m.descent))
        .unwrap_or((px2 * 0.75, -px2 * 0.25));
    let baseline2 = ((height as f32 * scale + ascent - descent) / 2.0) as i32;

    let mut cursor_x = x as f32;
    let mut prev: Option<char> = None;
    let ellipsis = '…';
    let mut chars_to_draw: Vec<char> = Vec::new();

    // Layout is measured at target resolution so the ellipsis math is correct.
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
            0.92,
        );
        cursor_x += metrics.advance_width;
        prev = Some(*ch);
    }

    // Composite the 2× mask down to the final buffer.
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
            let idx = ((ty as u32 * width + tx as u32) * 4) as usize;
            let inv = 1.0 - alpha;
            buffer[idx] = (243.0 * alpha + buffer[idx] as f32 * inv) as u8;
            buffer[idx + 1] = (244.0 * alpha + buffer[idx + 1] as f32 * inv) as u8;
            buffer[idx + 2] = (246.0 * alpha + buffer[idx + 2] as f32 * inv) as u8;
            let old_a = buffer[idx + 3] as f32 / 255.0;
            let new_a = alpha + old_a * inv;
            buffer[idx + 3] = (new_a * 255.0) as u8;
        }
    }
}

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
            // Glyphs rarely overlap, but a max keeps the mask physically plausible.
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

/// Resample recent audio samples into a fixed-length waveform buffer.
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
