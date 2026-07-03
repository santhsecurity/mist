use std::time::Instant;

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
/// Produces a 0x00RRGGBB pixel buffer with transparent pixels encoded as 0.
/// This keeps it compatible with `softbuffer` and makes it easy to convert to
/// RGBA for screenshots.
pub struct Renderer {
    width: u32,
    height: u32,
    state: OverlayState,
    levels: [f32; 12],
    smoothed: [f32; 12],
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
            levels: [0.0; 12],
            smoothed: [0.0; 12],
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

    pub fn set_levels(&mut self, levels: &[f32]) {
        let n = levels.len().min(self.levels.len());
        self.levels[..n].copy_from_slice(&levels[..n]);
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

    /// Render one frame and return the 0x00RRGGBB pixel buffer.
    pub fn render(&mut self) -> Vec<u32> {
        let width = self.width;
        let height = self.height;
        let mut buf = vec![0u32; (width * height) as usize];

        // Smooth levels toward the latest audio RMS values.
        let decay = 0.18;
        let attack = 0.42;
        for i in 0..self.smoothed.len() {
            let target = self.levels[i];
            let delta = target - self.smoothed[i];
            let factor = if delta > 0.0 { attack } else { decay };
            self.smoothed[i] += delta * factor;
        }

        let elapsed = self.start.elapsed().as_secs_f32();
        let overall = self.smoothed.iter().sum::<f32>() / self.smoothed.len().max(1) as f32;

        let radius = height.min(24);
        let shadow_radius = 10.0f32;

        // Background and shadow.
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let (dist, inside) = rounded_rect_distance(x, y, width, height, radius);

                if inside {
                    // Gradient background: slightly lighter at top.
                    let t = y as f32 / height as f32;
                    let top = Color::new(34, 34, 36);
                    let bot = Color::new(22, 22, 24);
                    let bg = top.lerp(&bot, t);

                    // Very subtle border near the edge.
                    let on_edge = dist > -1.5;
                    let pixel = if on_edge { Color::new(58, 58, 64) } else { bg };
                    buf[idx] = pixel.pack();
                } else if dist < shadow_radius {
                    let alpha = (1.0 - dist / shadow_radius).powi(2) * 0.45;
                    buf[idx] = blend(buf[idx], 0, 0, 0, alpha);
                }
            }
        }

        // State orb + glow.
        let (orb_r, orb_g, orb_b) = match self.state {
            OverlayState::Listening => (255, 70, 70),
            OverlayState::Processing => (255, 185, 50),
            OverlayState::Done => (55, 225, 120),
            OverlayState::Error => (255, 70, 70),
        };
        let pulse = if self.state == OverlayState::Listening {
            (elapsed * 2.8).sin() * 0.22 + 0.78 + overall * 0.30
        } else {
            0.92 + overall * 0.15
        };
        let orb_radius = (7.0 + overall * 4.0).min(10.0);
        let cx = 22.0;
        let cy = height as f32 / 2.0;
        draw_orb(
            &mut buf, width, height, cx, cy, orb_radius, pulse, orb_r, orb_g, orb_b, radius,
        );

        // Waveform bars.
        let bar_count = self.smoothed.len();
        let bar_w = 2u32;
        let gap = 3u32;
        let start_x = 42i32;
        let max_h = (height as i32 - 18).max(4);

        for i in 0..bar_count {
            let x = start_x + i as i32 * (bar_w as i32 + gap as i32);
            let level = self.smoothed[i];
            let bar_h = (max_h as f32 * (0.15 + level * 0.85)).min(max_h as f32) as i32;
            let bar_y = (height as i32 - bar_h) / 2;
            let brightness = (190.0 + level * 65.0) as u8;
            let cap_r = (bar_w / 2).min(1) as i32;

            for by in bar_y..(bar_y + bar_h) {
                let dist = (by - (bar_y + bar_h / 2)).abs() as f32 / (bar_h as f32 / 2.0 + 0.1);
                let alpha = (1.0 - dist * 0.5).max(0.45);
                let g = (brightness as f32 * alpha) as u8;
                let b = ((brightness + 25).min(255) as f32 * alpha) as u8;
                for bx in x..(x + bar_w as i32) {
                    if rounded_rect(bx as u32, by as u32, width, height, radius) {
                        let idx = (by as u32 * width + bx as u32) as usize;
                        buf[idx] = blend(buf[idx], g, g, b, 0.92);
                    }
                }
            }

            // Rounded cap at the top of the bar.
            if bar_h > cap_r as i32 * 2 {
                let cap_y = bar_y;
                let cap_cx = x + bar_w as i32 / 2;
                for dy in -cap_r as i32..=cap_r as i32 {
                    for dx in -cap_r as i32..=cap_r as i32 {
                        let px = cap_cx + dx;
                        let py = cap_y + dy;
                        if dx * dx + dy * dy <= cap_r as i32 * cap_r as i32 {
                            if rounded_rect(px as u32, py as u32, width, height, radius) {
                                let idx = (py as u32 * width + px as u32) as usize;
                                buf[idx] = blend(buf[idx], brightness, brightness, 255, 0.95);
                            }
                        }
                    }
                }
            }
        }

        // Text preview / result.
        if let Some(text) = &self.text {
            let text_area_x = start_x + bar_count as i32 * (bar_w as i32 + gap as i32) + 8;
            let max_text_w = width as i32 - text_area_x - 14;
            if max_text_w > 20 {
                // Slight dark halo for readability.
                draw_text(
                    &mut buf,
                    width,
                    height,
                    text_area_x + 1,
                    height as i32 / 2 + 1,
                    text,
                    14.0,
                    max_text_w,
                    &self.font,
                    radius,
                    Color::new(0, 0, 0),
                    0.55,
                );
                draw_text(
                    &mut buf,
                    width,
                    height,
                    text_area_x,
                    height as i32 / 2,
                    text,
                    14.0,
                    max_text_w,
                    &self.font,
                    radius,
                    Color::new(240, 240, 245),
                    0.95,
                );
            }
        }

        buf
    }
}

fn draw_orb(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    radius: f32,
    pulse: f32,
    r: u8,
    g: u8,
    b: u8,
    corner_radius: u32,
) {
    let glow_r = radius + 5.0;
    let min_x = (cx - glow_r).max(0.0) as i32;
    let max_x = (cx + glow_r).min(width as f32) as i32;
    let min_y = (cy - glow_r).max(0.0) as i32;
    let max_y = (cy + glow_r).min(height as f32) as i32;

    for y in min_y..max_y {
        for x in min_x..max_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= glow_r && rounded_rect(x as u32, y as u32, width, height, corner_radius) {
                let idx = (y as u32 * width + x as u32) as usize;
                let glow_alpha = (1.0 - (dist / glow_r)).max(0.0) * 0.40 * pulse;
                buffer[idx] = blend(buffer[idx], r, g, b, glow_alpha as f32);
                if dist <= radius {
                    // Radial gradient core.
                    let t = dist / radius;
                    let core =
                        Color::new(r, g, b).lerp(&Color::new(255, 255, 255), (1.0 - t) * 0.35);
                    let core_alpha = (1.0 - t).max(0.0);
                    buffer[idx] = blend(buffer[idx], core.r, core.g, core.b, core_alpha as f32);
                }
            }
        }
    }
}

fn draw_text(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    _y: i32,
    text: &str,
    px: f32,
    max_width: i32,
    font: &Option<fontdue::Font>,
    corner_radius: u32,
    color: Color,
    alpha_mul: f32,
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

    // First pass: measure what fits.
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

    cursor_x = x as f32;
    for ch in chars_to_draw {
        if let Some(k) = prev.and_then(|p| font.horizontal_kern(p, ch, px)) {
            cursor_x += k;
        }
        let (metrics, bitmap) = font.rasterize(ch, px);
        let gx = cursor_x as i32 + metrics.xmin;
        let gy = baseline + metrics.ymin;
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let px_x = gx + col as i32;
                let px_y = gy + row as i32;
                if px_x < 0 || px_x >= width as i32 || px_y < 0 || px_y >= height as i32 {
                    continue;
                }
                if !rounded_rect(px_x as u32, px_y as u32, width, height, corner_radius) {
                    continue;
                }
                let alpha = bitmap[row * metrics.width + col] as f32 / 255.0 * alpha_mul;
                let idx = (px_y as u32 * width + px_x as u32) as usize;
                buffer[idx] = blend(buffer[idx], color.r, color.g, color.b, alpha);
            }
        }
        cursor_x += metrics.advance_width;
        prev = Some(ch);
    }
}

/// Compute per-band RMS levels from the most recent audio samples.
pub fn audio_levels(samples: &[f32], bands: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; bands];
    if samples.is_empty() || bands == 0 {
        return out;
    }
    // Use the last 0.5s of audio at 16 kHz.
    let window = (16000.0 * 0.5) as usize;
    let start = samples.len().saturating_sub(window);
    let recent = &samples[start..];
    let band_size = recent.len() / bands;
    if band_size == 0 {
        return out;
    }
    for (i, level) in out.iter_mut().enumerate() {
        let s = i * band_size;
        let e = ((i + 1) * band_size).min(recent.len());
        let slice = &recent[s..e];
        if slice.is_empty() {
            continue;
        }
        let rms = (slice.iter().map(|v| v * v).sum::<f32>() / slice.len() as f32).sqrt();
        // Whisper input is usually well below 0 dBFS; scale gently.
        *level = (rms * 8.0).min(1.0);
    }
    out
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

/// Signed distance from (x, y) to the edge of a rounded rectangle.
/// Negative values are inside, positive outside.
fn rounded_rect_distance(x: u32, y: u32, w: u32, h: u32, r: u32) -> (f32, bool) {
    let r = r.min(w / 2).min(h / 2) as f32;
    let px = x as f32;
    let py = y as f32;
    let cx = (w as f32 - 1.0) / 2.0;
    let cy = (h as f32 - 1.0) / 2.0;
    let dx = (px - cx).abs() - (w as f32 / 2.0 - 1.0 - r);
    let dy = (py - cy).abs() - (h as f32 / 2.0 - 1.0 - r);
    let outside = dx.max(dy);
    let inside_dist = if outside > 0.0 {
        let d = outside.hypot(0.0);
        d
    } else {
        outside.max(-r)
    };
    let inside = dx <= 0.0 || dy <= 0.0 || (dx * dx + dy * dy <= r * r);
    (inside_dist, inside)
}

/// Legacy inside-test used by text/glyph clipping.
fn rounded_rect(x: u32, y: u32, w: u32, h: u32, r: u32) -> bool {
    rounded_rect_distance(x, y, w, h, r).1
}

#[derive(Clone, Copy)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
}

impl Color {
    fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
    fn pack(&self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }
    fn lerp(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: (self.r as f32 * (1.0 - t) + other.r as f32 * t) as u8,
            g: (self.g as f32 * (1.0 - t) + other.g as f32 * t) as u8,
            b: (self.b as f32 * (1.0 - t) + other.b as f32 * t) as u8,
        }
    }
}

/// Alpha blend a color onto a 0x00RRGGBB background pixel.
fn blend(dst: u32, r: u8, g: u8, b: u8, alpha: f32) -> u32 {
    let a = alpha.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    let dr = ((dst >> 16) & 0xff) as f32;
    let dg = ((dst >> 8) & 0xff) as f32;
    let db = (dst & 0xff) as f32;
    let rr = (r as f32 * a + dr * inv) as u32;
    let rg = (g as f32 * a + dg * inv) as u32;
    let rb = (b as f32 * a + db * inv) as u32;
    (rr << 16) | (rg << 8) | rb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_levels_empty() {
        assert!(audio_levels(&[], 4).iter().all(|&v| v == 0.0));
    }

    #[test]
    fn audio_levels_sine() {
        let samples: Vec<f32> = (0..16000)
            .map(|i| (i as f32 / 16000.0 * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5)
            .collect();
        let levels = audio_levels(&samples, 4);
        assert!(levels.iter().all(|&v| v > 0.05 && v <= 1.0));
    }

    #[test]
    fn rounded_rect_corners() {
        assert!(rounded_rect(10, 10, 48, 20, 8));
        assert!(!rounded_rect(1, 1, 48, 20, 8));
    }
}
