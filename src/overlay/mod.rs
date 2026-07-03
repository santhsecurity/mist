use anyhow::Result;
use std::time::{Duration, Instant};
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event_loop::EventLoop;
use tao::window::{Window, WindowBuilder};

mod renderer;
pub use renderer::{waveform_from_samples, OverlayState, Renderer};

pub struct Overlay {
    window: Window,
    surface: softbuffer::Surface<&'static Window, &'static Window>,
    renderer: Renderer,
    show_until: Option<Instant>,
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
        let width = 400;
        let height = 56;
        let window = WindowBuilder::new()
            .with_decorations(false)
            .with_always_on_top(true)
            .with_visible(false)
            .with_inner_size(LogicalSize::new(width as f64, height as f64))
            .with_transparent(true)
            .with_resizable(false)
            .with_title("Mist")
            .build(event_loop)?;

        let leaked = leak_window_and_context(window)?;
        let surface = softbuffer::Surface::new(leaked.context, leaked.window)
            .map_err(|e| anyhow::anyhow!("softbuffer surface: {}", e))?;

        let window_ref: &Window = leaked.window;

        Ok(Self {
            window: unsafe { std::ptr::read(window_ref as *const Window) },
            surface,
            renderer: Renderer::new(width, height),
            show_until: None,
        })
    }

    pub fn show_near_cursor(&mut self) {
        self.renderer.set_state(OverlayState::Listening);
        self.renderer.clear_text();
        self.renderer.reset_time();
        self.show_until = None;

        let offset = 18;
        let (mx, my) = match mouse_position::mouse_position::Mouse::get_mouse_position() {
            mouse_position::mouse_position::Mouse::Position { x, y } => (x, y),
            mouse_position::mouse_position::Mouse::Error => {
                // Fallback: top-right of the primary monitor.
                if let Some(monitor) = self.window.current_monitor() {
                    let size = monitor.size();
                    let pos = monitor.position();
                    (
                        pos.x + size.width as i32 - (self.renderer.width() as i32 + 20),
                        pos.y + 24,
                    )
                } else {
                    (100, 100)
                }
            }
        };

        let size = self.window.inner_size();
        let pos = clamp_to_monitor(
            &self.window,
            mx + offset,
            my + offset,
            size.width as i32,
            size.height as i32,
        );

        self.window.set_outer_position(pos);
        self.window.set_visible(true);
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    pub fn set_state(&mut self, state: OverlayState) {
        self.renderer.set_state(state);
    }

    pub fn set_waveform_samples(&mut self, samples: &[f32]) {
        self.renderer.set_waveform_samples(samples);
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.renderer.set_text(text);
    }

    pub fn dismiss_after(&mut self, duration: Duration) {
        self.show_until = Some(Instant::now() + duration);
    }

    pub fn should_dismiss(&self) -> bool {
        self.show_until
            .map(|t| Instant::now() >= t)
            .unwrap_or(false)
    }

    pub fn draw(&mut self) -> Result<()> {
        let size = self.window.inner_size();
        let width = size.width as u32;
        let height = size.height as u32;
        if width == 0 || height == 0 {
            return Ok(());
        }

        // Keep the renderer size in sync with the window.
        if self.renderer.width() != width || self.renderer.height() != height {
            self.renderer = Renderer::new(width, height).with_font(self.renderer.take_font());
        }

        let rgba = self.renderer.render();
        self.blit(&rgba, width, height)
    }

    fn blit(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<()> {
        use std::num::NonZeroU32;
        let w_nz = NonZeroU32::new(width).unwrap();
        let h_nz = NonZeroU32::new(height).unwrap();
        self.surface
            .resize(w_nz, h_nz)
            .map_err(|e| anyhow::anyhow!("resize: {}", e))?;
        let mut sb = self
            .surface
            .buffer_mut()
            .map_err(|e| anyhow::anyhow!("buffer: {}", e))?;

        // Convert unpremultiplied RGBA to 0x00RRGGBB for softbuffer.
        // softbuffer does not support per-pixel alpha; 0 is transparent,
        // everything else is opaque.
        let u32_buf: Vec<u32> = rgba
            .chunks_exact(4)
            .map(|c| {
                if c[3] == 0 {
                    0
                } else {
                    ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32)
                }
            })
            .collect();
        sb.copy_from_slice(&u32_buf);
        sb.present()
            .map_err(|e| anyhow::anyhow!("present: {}", e))?;
        Ok(())
    }
}

fn clamp_to_monitor(
    window: &Window,
    mut x: i32,
    mut y: i32,
    win_w: i32,
    win_h: i32,
) -> PhysicalPosition<i32> {
    if let Some(monitor) = window.current_monitor() {
        let size = monitor.size();
        let pos = monitor.position();
        let mx = pos.x;
        let my = pos.y;
        let mw = size.width as i32;
        let mh = size.height as i32;
        if x + win_w > mx + mw {
            x = mx + mw - win_w - 8;
        }
        if y + win_h > my + mh {
            y = my + mh - win_h - 8;
        }
        x = x.max(mx + 4);
        y = y.max(my + 4);
    }
    PhysicalPosition::new(x, y)
}
