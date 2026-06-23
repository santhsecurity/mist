use anyhow::Result;
use std::num::NonZeroU32;
use std::time::Instant;
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event_loop::EventLoop;
use tao::window::{Window, WindowBuilder};

pub struct Overlay {
    window: Window,
    surface: softbuffer::Surface<&'static Window, &'static Window>,
    start: Instant,
}

// softbuffer needs &Window references that outlive the Surface. Since the
// overlay lives for the entire process, we leak the window + context into
// 'static references. There is exactly one of each, and they are never freed.
struct Leaked {
    window: &'static Window,
    context: &'static softbuffer::Context<&'static Window>,
}

fn leak_window_and_context(window: Window) -> Result<Leaked> {
    let window: &'static Window = Box::leak(Box::new(window));
    let context = softbuffer::Context::new(window)
        .map_err(|e| anyhow::anyhow!("softbuffer context: {}", e))?;
    let context: &'static softbuffer::Context<&'static Window> = Box::leak(Box::new(context));
    Ok(Leaked { window, context })
}

impl Overlay {
    pub fn new(event_loop: &EventLoop<()>) -> Result<Self> {
        let window = WindowBuilder::new()
            .with_decorations(false)
            .with_always_on_top(true)
            .with_visible(false)
            .with_inner_size(LogicalSize::new(260.0, 48.0))
            .with_transparent(true)
            .with_resizable(false)
            .with_title("Flow")
            .build(event_loop)?;

        let leaked = leak_window_and_context(window)?;
        let surface = softbuffer::Surface::new(leaked.context, leaked.window)
            .map_err(|e| anyhow::anyhow!("softbuffer surface: {}", e))?;

        // We need to keep a usable Window handle for show/hide/position.
        // Since the window is leaked, we can reconstruct a reference from the
        // leaked pointer. This is safe because the leaked reference is 'static.
        let window_ref: &Window = leaked.window;

        Ok(Self {
            // SAFETY: window_ref points to leaked memory that lives for the
            // entire process. We create a Window value by reading the pointer.
            // The original leaked reference is never deallocated.
            window: unsafe { std::ptr::read(window_ref as *const Window) },
            surface,
            start: Instant::now(),
        })
    }

    pub fn show(&mut self) {
        self.start = Instant::now();
        self.position_top_right();
        self.window.set_visible(true);
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    fn position_top_right(&self) {
        if let Some(monitor) = self.window.current_monitor() {
            let size = monitor.size();
            let pos = monitor.position();
            self.window.set_outer_position(PhysicalPosition::new(
                pos.x + size.width as i32 - 280,
                pos.y + 20,
            ));
        }
    }

    pub fn draw(&mut self) -> Result<()> {
        let size = self.window.inner_size();
        let width = size.width as u32;
        let height = size.height as u32;
        if width == 0 || height == 0 {
            return Ok(());
        }

        let w_nz = NonZeroU32::new(width).unwrap();
        let h_nz = NonZeroU32::new(height).unwrap();
        self.surface
            .resize(w_nz, h_nz)
            .map_err(|e| anyhow::anyhow!("resize: {}", e))?;
        let mut buffer = self.surface
            .buffer_mut()
            .map_err(|e| anyhow::anyhow!("buffer: {}", e))?;

        let elapsed = self.start.elapsed().as_secs_f32();

        // Fade-in over the first 200ms.
        let fade = (elapsed / 0.2).min(1.0);

        // --- Background: rounded dark pill ---
        let radius = height.min(24);
        for y in 0..height {
            for x in 0..width {
                let inside = rounded_rect(x, y, width, height, radius);
                buffer[(y * width + x) as usize] = if inside {
                    let alpha = (22.0 * fade) as u8;
                    pixel(alpha, alpha, alpha + 2)
                } else {
                    pixel(0, 0, 0)
                };
            }
        }

        // --- Pulsing red recording dot (left side) ---
        let pulse = (elapsed * 3.0).sin() * 0.3 + 0.7;
        let dot_r = (220.0 * pulse) as u8;
        let cx = 22i32;
        let cy = height as i32 / 2;
        let dot_radius = 8i32;

        // Glow ring.
        let glow_radius = dot_radius + 3;
        for y in 0..height {
            for x in 0..width {
                let dx = x as i32 - cx;
                let dy = y as i32 - cy;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= glow_radius * glow_radius && dist_sq > dot_radius * dot_radius {
                    if rounded_rect(x, y, width, height, radius) {
                        let glow_r = (dot_r as f32 * 0.4 * fade) as u8;
                        buffer[(y * width + x) as usize] = pixel(glow_r, 12, 12);
                    }
                }
            }
        }
        // Solid dot.
        for y in 0..height {
            for x in 0..width {
                let dx = x as i32 - cx;
                let dy = y as i32 - cy;
                if dx * dx + dy * dy <= dot_radius * dot_radius {
                    buffer[(y * width + x) as usize] = pixel(dot_r, 30, 30);
                }
            }
        }

        // --- Animated waveform bars ---
        let bar_count = 16;
        let bar_width = 3u32;
        let bar_gap = 4u32;
        let start_x = 44i32;
        let max_bar_height = (height as i32 - 14).max(4);

        for i in 0..bar_count {
            let x = start_x + i as i32 * (bar_width as i32 + bar_gap as i32);
            let phase = elapsed * 5.5 + i as f32 * 0.65;
            let height_factor = (phase.sin() * 0.5 + 0.5) * 0.6 + 0.12;
            let bar_h = (max_bar_height as f32 * height_factor) as i32;
            let bar_y = (height as i32 - bar_h) / 2;

            for by in bar_y..(bar_y + bar_h) {
                let center_dist =
                    ((by - (bar_y + bar_h / 2)) as f32).abs() / (bar_h as f32 / 2.0 + 0.1);
                let brightness = ((1.0 - center_dist * 0.4) * fade).max(0.0);
                let g = (210.0 * brightness) as u8;
                let b = (220.0 * brightness) as u8;
                for bx in x..(x + bar_width as i32) {
                    if bx >= 0 && bx < width as i32 && by >= 0 && by < height as i32 {
                        if rounded_rect(bx as u32, by as u32, width, height, radius) {
                            buffer[(by as u32 * width + bx as u32) as usize] = pixel(g, g, b);
                        }
                    }
                }
            }
        }

        // --- "REC" pixel-font label ---
        let text_x = start_x + bar_count as i32 * (bar_width as i32 + bar_gap as i32) + 8;
        let text_y = height as i32 / 2 - 3;
        draw_rec_text(&mut buffer, width, height, text_x, text_y, dot_r, radius);

        buffer
            .present()
            .map_err(|e| anyhow::anyhow!("present: {}", e))?;
        Ok(())
    }
}

/// Draw "REC" in 5x7 pixel font at the given position.
fn draw_rec_text(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    r: u8,
    corner_radius: u32,
) {
    #[rustfmt::skip]
    const R: [[u8; 5]; 7] = [
        [1,1,1,1,0], [1,0,0,0,1], [1,0,0,0,1], [1,1,1,1,0],
        [1,0,1,0,0], [1,0,0,1,0], [1,0,0,0,1],
    ];
    #[rustfmt::skip]
    const E: [[u8; 5]; 7] = [
        [1,1,1,1,1], [1,0,0,0,0], [1,0,0,0,0], [1,1,1,1,0],
        [1,0,0,0,0], [1,0,0,0,0], [1,1,1,1,1],
    ];
    #[rustfmt::skip]
    const C: [[u8; 5]; 7] = [
        [0,1,1,1,0], [1,0,0,0,1], [1,0,0,0,0], [1,0,0,0,0],
        [1,0,0,0,0], [1,0,0,0,1], [0,1,1,1,0],
    ];

    let letters = [&R, &E, &C];
    let mut ox = x;
    for letter in &letters {
        for (row, line) in letter.iter().enumerate() {
            for (col, &on) in line.iter().enumerate() {
                if on == 1 {
                    let px = ox + col as i32;
                    let py = y + row as i32;
                    if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                        if rounded_rect(px as u32, py as u32, width, height, corner_radius) {
                            buffer[(py as u32 * width + px as u32) as usize] = pixel(r, 60, 60);
                        }
                    }
                }
            }
        }
        ox += 6;
    }
}

/// Test if (x, y) is inside a rounded rectangle.
fn rounded_rect(x: u32, y: u32, w: u32, h: u32, r: u32) -> bool {
    let r = r.min(w / 2).min(h / 2);
    if x < r && y < r {
        let dx = r - x;
        let dy = r - y;
        return dx * dx + dy * dy <= r * r;
    }
    if x >= w - r && y < r {
        let dx = x - (w - r - 1);
        let dy = r - y;
        return dx * dx + dy * dy <= r * r;
    }
    if x < r && y >= h - r {
        let dx = r - x;
        let dy = y - (h - r - 1);
        return dx * dx + dy * dy <= r * r;
    }
    if x >= w - r && y >= h - r {
        let dx = x - (w - r - 1);
        let dy = y - (h - r - 1);
        return dx * dx + dy * dy <= r * r;
    }
    true
}

/// Pack RGB into softbuffer's native 0x00RRGGBB pixel format.
#[inline(always)]
fn pixel(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | (b as u32)
}
