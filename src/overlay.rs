use anyhow::Result;

#[cfg(target_os = "linux")]
mod inner {
    use super::*;

    pub struct Overlay;

    impl Overlay {
        pub fn new(_event_loop: &tao::event_loop::EventLoop<()>) -> Result<Self> {
            Ok(Self)
        }

        pub fn show(&mut self) {}
        pub fn hide(&self) {}
        pub fn draw(&mut self) -> Result<()> {
            Ok(())
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod inner {
    use super::*;
    use std::time::Instant;
    use tao::dpi::{LogicalSize, PhysicalPosition};
    use tao::event_loop::EventLoop;
    use tao::window::{Window, WindowBuilder};

    pub struct Overlay {
        window: Window,
        surface: softbuffer::Surface,
        start: Instant,
    }

    impl Overlay {
        pub fn new(event_loop: &EventLoop<()>) -> Result<Self> {
            let window = WindowBuilder::new()
                .with_decorations(false)
                .with_always_on_top(true)
                .with_visible(false)
                .with_inner_size(LogicalSize::new(220.0, 50.0))
                .with_transparent(true)
                .with_resizable(false)
                .build(event_loop)?;

            let context = softbuffer::Context::new(&window)?;
            let surface = softbuffer::Surface::new(&context, &window)?;

            Ok(Self {
                window,
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
                    pos.x + size.width as i32 - 240,
                    pos.y + 20,
                ));
            }
        }

        pub fn draw(&mut self) -> Result<()> {
            let size = self.window.inner_size();
            let width = size.width as u32;
            let height = size.height as u32;

            self.surface.resize(width, height)?;
            let mut buffer = self.surface.buffer_mut()?;

            let elapsed = self.start.elapsed().as_secs_f32();

            // Background: dark pill
            for y in 0..height {
                for x in 0..width {
                    buffer[(y * width + x) as usize] = pixel(25, 25, 25, 240);
                }
            }

            // Pulsing red circle (left side)
            let pulse = (elapsed * 3.0).sin() * 0.35 + 0.65;
            let r = (255.0 * pulse) as u8;
            let cx = 22;
            let cy = height as i32 / 2;
            let radius = 9;

            for y in 0..height {
                for x in 0..width {
                    let dx = x as i32 - cx;
                    let dy = y as i32 - cy;
                    if dx * dx + dy * dy <= radius * radius {
                        buffer[(y * width + x) as usize] = pixel(r, 25, 25, 255);
                    }
                }
            }

            // Animated waveform bars
            let bar_count = 14;
            let bar_width = 3;
            let bar_gap = 3;
            let start_x = 46;
            let max_bar_height = (height - 16) as i32;

            for i in 0..bar_count {
                let x = start_x + i * (bar_width + bar_gap);
                let phase = elapsed * 5.0 + i as f32 * 0.7;
                let height_factor = (phase.sin() * 0.5 + 0.5) * 0.55 + 0.15;
                let bar_height = (max_bar_height as f32 * height_factor) as i32;
                let bar_y = (height as i32 - bar_height) / 2;

                for by in bar_y..(bar_y + bar_height) {
                    for bx in x..(x + bar_width) {
                        if bx >= 0 && bx < width as i32 && by >= 0 && by < height as i32 {
                            buffer[(by as u32 * width + bx as u32) as usize] =
                                pixel(200, 200, 200, 220);
                        }
                    }
                }
            }

            buffer.present()?;
            Ok(())
        }
    }

    #[cfg(target_os = "macos")]
    fn pixel(r: u8, g: u8, b: u8, a: u8) -> u32 {
        (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | (b as u32)
    }

    #[cfg(target_os = "windows")]
    fn pixel(r: u8, g: u8, b: u8, a: u8) -> u32 {
        (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | (b as u32)
    }
}

pub use inner::Overlay;
