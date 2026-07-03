use std::time::Instant;
use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint, PathBuilder, Pixmap,
    Point, SpreadMode, Stroke, Transform,
};

/// Visual state of the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayState {
    Listening,
    Processing,
    Done,
    Error,
}

/// Software renderer for the Mist overlay pill.
///
/// Produces an unpremultiplied RGBA buffer. Transparent pixels are encoded as
/// `(0,0,0,0)`. This makes it easy to composite onto the desktop or save as a
/// PNG.
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
            samples: vec![0.0; 160],
            smoothed: vec![0.0; 160],
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

    /// Replace the current waveform samples. The renderer will smoothly
    /// interpolate toward these values each frame.
    pub fn set_waveform_samples(&mut self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        // Resample to a fixed number of points so the path density is stable.
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
        let mut pixmap =
            Pixmap::new(self.width, self.height).unwrap_or_else(|| Pixmap::new(1, 1).unwrap());

        // Smooth the waveform samples for fluid motion.
        let attack = 0.35;
        let decay = 0.12;
        for i in 0..self.smoothed.len() {
            let target = self.samples.get(i).copied().unwrap_or(0.0);
            let delta = target - self.smoothed[i];
            self.smoothed[i] += delta * if delta > 0.0 { attack } else { decay };
        }

        let elapsed = self.start.elapsed().as_secs_f32();

        let rect = rounded_rect_path(0.0, 0.0, self.width as f32, self.height as f32, 16.0);

        // Soft shadow behind the pill.
        // tiny-skia doesn't have blur masks, so we fake a soft shadow by
        // drawing a slightly larger, dark shape behind the pill.
        let shadow_paint = {
            let mut p = Paint::default();
            p.set_color_rgba8(0, 0, 0, 40);
            p.anti_alias = true;
            p
        };
        let shadow_path = rounded_rect_path(
            -2.0,
            2.0,
            self.width as f32 + 4.0,
            self.height as f32 + 4.0,
            18.0,
        );
        pixmap.fill_path(
            &shadow_path,
            &shadow_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        // Gradient pill background.
        let bg_paint = {
            let gradient = LinearGradient::new(
                Point::from_xy(0.0, 0.0),
                Point::from_xy(0.0, self.height as f32),
                vec![
                    GradientStop::new(0.0, Color::from_rgba8(38, 38, 42, 235)),
                    GradientStop::new(1.0, Color::from_rgba8(24, 24, 27, 235)),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            )
            .unwrap();
            let mut p = Paint::default();
            p.shader = gradient;
            p.anti_alias = true;
            p
        };
        pixmap.fill_path(
            &rect,
            &bg_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        // Subtle border.
        let mut border_stroke = Stroke::default();
        border_stroke.width = 1.0;
        border_stroke.line_cap = LineCap::Round;
        border_stroke.line_join = LineJoin::Round;
        let mut border_paint = Paint::default();
        border_paint.set_color_rgba8(255, 255, 255, 22);
        border_paint.anti_alias = true;
        pixmap.stroke_path(
            &rect,
            &border_paint,
            &border_stroke,
            Transform::identity(),
            None,
        );

        // Orb.
        draw_orb(
            &mut pixmap,
            self.state,
            elapsed,
            22.0,
            self.height as f32 / 2.0,
        );

        // Waveform.
        let text_area_x = self.width as f32 - 16.0;
        let wave_end = text_area_x - 10.0;
        draw_waveform(
            &mut pixmap,
            &self.smoothed,
            self.state,
            elapsed,
            46.0,
            wave_end,
            self.height as f32,
        );

        // Text.
        let mut rgba = pixmap_to_rgba(&pixmap);
        if let Some(text) = &self.text {
            // Waveform occupies x=46..=46+120. Text starts after that with some
            // padding, up to the right edge.
            let text_x = 46 + 120 + 12;
            let max_text_w = self.width as i32 - text_x - 16;
            draw_text_rgba(
                &mut rgba,
                self.width,
                self.height,
                text_x,
                self.height as i32 / 2,
                text,
                15.0,
                max_text_w,
                &self.font,
            );
        }
        rgba
    }
}

fn draw_orb(pixmap: &mut Pixmap, state: OverlayState, elapsed: f32, cx: f32, cy: f32) {
    let (r, g, b) = match state {
        OverlayState::Listening => (255, 75, 75),
        OverlayState::Processing => (255, 185, 60),
        OverlayState::Done => (55, 220, 115),
        OverlayState::Error => (255, 60, 60),
    };

    let pulse = if state == OverlayState::Listening {
        (elapsed * 2.8).sin() * 0.18 + 0.82
    } else {
        0.95
    };

    let radius = 7.5;

    // Glow (faked with concentric translucent circles).
    for i in 1..=6 {
        let rr = radius + i as f32 * 1.2;
        let alpha = ((50.0 / i as f32) * pulse) as u8;
        let path = {
            let mut pb = PathBuilder::new();
            pb.push_circle(cx, cy, rr);
            pb.finish().unwrap()
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, alpha);
        paint.anti_alias = true;
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    // Core with radial gradient.
    let core_path = {
        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, radius);
        pb.finish().unwrap()
    };
    let gradient = tiny_skia::RadialGradient::new(
        Point::from_xy(cx - 2.0, cy - 2.0),
        0.0,
        Point::from_xy(cx, cy),
        radius,
        vec![
            GradientStop::new(0.0, Color::from_rgba8(255, 255, 255, 240)),
            GradientStop::new(0.5, Color::from_rgba8(r, g, b, 240)),
            GradientStop::new(
                1.0,
                Color::from_rgba8(
                    (r as f32 * 0.6) as u8,
                    (g as f32 * 0.6) as u8,
                    (b as f32 * 0.6) as u8,
                    240,
                ),
            ),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();
    let mut core_paint = Paint::default();
    core_paint.shader = gradient;
    core_paint.anti_alias = true;
    pixmap.fill_path(
        &core_path,
        &core_paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

fn draw_waveform(
    pixmap: &mut Pixmap,
    samples: &[f32],
    state: OverlayState,
    elapsed: f32,
    x_start: f32,
    x_end: f32,
    height: f32,
) {
    if samples.len() < 2 {
        return;
    }

    // For a more organic look, add a travelling phase to the wave.
    let phase = elapsed * 12.0;
    let center = height / 2.0;
    let max_amp = (height / 2.0) - 10.0;

    // Compute a perceived loudness to scale the wave.
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    let loudness = (rms * 6.0).min(1.0);

    let points: Vec<(f32, f32)> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let t = i as f32 / (samples.len() - 1) as f32;
            let x = x_start + t * (x_end - x_start);
            let carrier = (t * 24.0 + phase).sin() * 0.4 + 0.6;
            let amp = s * carrier * max_amp * (0.25 + 0.75 * loudness);
            let y = center - amp.clamp(-max_amp, max_amp);
            (x, y)
        })
        .collect();

    let path = smooth_path(&points);

    let (r, g, b) = match state {
        OverlayState::Listening => (80, 220, 255),
        OverlayState::Processing => (255, 200, 80),
        _ => (180, 180, 190),
    };

    // Glow simulation: draw several increasingly thin, bright strokes.
    for (width, alpha) in [(6.0, 40), (4.0, 70), (2.5, 150)] {
        let mut stroke = Stroke::default();
        stroke.width = width;
        stroke.line_cap = LineCap::Round;
        stroke.line_join = LineJoin::Round;
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, (alpha as f32 * (0.3 + 0.7 * loudness)) as u8);
        paint.anti_alias = true;
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    // Bright core.
    let mut core_stroke = Stroke::default();
    core_stroke.width = 1.5;
    core_stroke.line_cap = LineCap::Round;
    core_stroke.line_join = LineJoin::Round;
    let mut core_paint = Paint::default();
    core_paint.set_color_rgba8(255, 255, 255, 220);
    core_paint.anti_alias = true;
    pixmap.stroke_path(
        &path,
        &core_paint,
        &core_stroke,
        Transform::identity(),
        None,
    );
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

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let mut pb = PathBuilder::new();
    let r = r.min(w / 2.0).min(h / 2.0);
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
            // Unpremultiply.
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
    let ascent = font
        .horizontal_line_metrics(px)
        .map(|m| m.ascent)
        .unwrap_or(px * 0.75);
    let baseline = (height as i32 + ascent as i32) / 2;

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

    // Dark halo.
    {
        cursor_x = x as f32;
        for ch in &chars_to_draw {
            if let Some(k) = prev.and_then(|p| font.horizontal_kern(p, *ch, px)) {
                cursor_x += k;
            }
            let (metrics, bitmap) = font.rasterize(*ch, px);
            let gx = cursor_x as i32 + metrics.xmin + 1;
            let gy = baseline + metrics.ymin + 1;
            blit_glyph_rgba(
                buffer,
                width,
                height,
                gx,
                gy,
                &bitmap,
                metrics.width,
                metrics.height,
                0,
                0,
                0,
                0.45,
            );
            cursor_x += metrics.advance_width;
            prev = Some(*ch);
        }
    }

    // Main text.
    cursor_x = x as f32;
    prev = None;
    for ch in &chars_to_draw {
        if let Some(k) = prev.and_then(|p| font.horizontal_kern(p, *ch, px)) {
            cursor_x += k;
        }
        let (metrics, bitmap) = font.rasterize(*ch, px);
        let gx = cursor_x as i32 + metrics.xmin;
        let gy = baseline + metrics.ymin;
        blit_glyph_rgba(
            buffer,
            width,
            height,
            gx,
            gy,
            &bitmap,
            metrics.width,
            metrics.height,
            240,
            240,
            245,
            0.95,
        );
        cursor_x += metrics.advance_width;
        prev = Some(*ch);
    }
}

fn blit_glyph_rgba(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    gx: i32,
    gy: i32,
    bitmap: &[u8],
    w: usize,
    h: usize,
    r: u8,
    g: u8,
    b: u8,
    alpha_mul: f32,
) {
    for row in 0..h {
        for col in 0..w {
            let px = gx + col as i32;
            let py = gy + row as i32;
            if px < 0 || px >= width as i32 || py < 0 || py >= height as i32 {
                continue;
            }
            let alpha = bitmap[row * w + col] as f32 / 255.0 * alpha_mul;
            let idx = ((py as u32 * width + px as u32) * 4) as usize;
            let inv = 1.0 - alpha;
            buffer[idx] = (r as f32 * alpha + buffer[idx] as f32 * inv) as u8;
            buffer[idx + 1] = (g as f32 * alpha + buffer[idx + 1] as f32 * inv) as u8;
            buffer[idx + 2] = (b as f32 * alpha + buffer[idx + 2] as f32 * inv) as u8;
            let old_a = buffer[idx + 3] as f32 / 255.0;
            let new_a = alpha + old_a * inv;
            buffer[idx + 3] = (new_a * 255.0) as u8;
        }
    }
}

fn load_font() -> Option<fontdue::Font> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    let families: Vec<fontdb::Family> = vec![
        fontdb::Family::Name("Inter"),
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
                fontdue::Font::from_bytes(data.to_vec(), fontdue::FontSettings::default()).ok()
            }) {
                return font;
            }
        }
    }
    None
}

/// Compute resampled waveform points from the most recent audio samples.
pub fn waveform_from_samples(samples: &[f32], target_len: usize) -> Vec<f32> {
    if samples.is_empty() || target_len == 0 {
        return vec![0.0; target_len];
    }
    let window = (16000.0 * 0.25) as usize; // 250ms
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
