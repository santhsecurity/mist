use anyhow::Result;
use log::info;
use std::sync::OnceLock;

/// The detected typing backend on Linux, cached so we don't re-probe on every
/// paste call.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
enum LinuxTypingBackend {
    Xdotool,
    Wtype,
    Ydotool,
}

/// Returns whether a typing backend is available on this platform.
pub fn typing_backend_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        detect_linux_backend().is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // macOS and Windows use enigo, which is compiled in.
        true
    }
}

/// The detected typing backend on Linux, cached for the process lifetime.
///
/// Detection runs once on first paste. If no typing tool is installed at
/// startup, the `None` result is cached permanently — installing xdotool
/// later won't take effect until Mist is restarted. This is intentional for
/// a daemon: the environment shouldn't change under a running process.
#[cfg(target_os = "linux")]
static LINUX_BACKEND: OnceLock<Option<LinuxTypingBackend>> = OnceLock::new();

/// Detect the display server and available typing tool once.
#[cfg(target_os = "linux")]
fn detect_linux_backend() -> Option<LinuxTypingBackend> {
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let wayland = session_type == "wayland" || std::env::var("WAYLAND_DISPLAY").is_ok();

    // On Wayland, prefer wtype → ydotool; skip xdotool (it doesn't work).
    // On X11, prefer xdotool → ydotool → wtype.
    let candidates: &[(&str, LinuxTypingBackend)] = if wayland {
        &[
            ("wtype", LinuxTypingBackend::Wtype),
            ("ydotool", LinuxTypingBackend::Ydotool),
        ]
    } else {
        &[
            ("xdotool", LinuxTypingBackend::Xdotool),
            ("ydotool", LinuxTypingBackend::Ydotool),
            ("wtype", LinuxTypingBackend::Wtype),
        ]
    };

    for (cmd, backend) in candidates {
        if std::process::Command::new(cmd)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            let server = if wayland { "Wayland" } else { "X11" };
            info!("Detected {} session, using {} for text input", server, cmd);
            return Some(*backend);
        }
    }

    None
}

/// Type text directly at the current cursor position.
/// No clipboard involved — text appears exactly as if typed on a keyboard.
pub fn paste_text(text: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    return type_macos(text);

    #[cfg(target_os = "windows")]
    return type_windows(text);

    #[cfg(target_os = "linux")]
    return type_linux(text);

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    anyhow::bail!("Unsupported platform");
}

#[cfg(target_os = "macos")]
fn type_macos(text: &str) -> Result<()> {
    use enigo::{Enigo, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.text(text)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn type_windows(text: &str) -> Result<()> {
    use enigo::{Enigo, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.text(text)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn type_linux(text: &str) -> Result<()> {
    let backend = LINUX_BACKEND.get_or_init(detect_linux_backend);

    match backend {
        Some(LinuxTypingBackend::Xdotool) => {
            let status = std::process::Command::new("xdotool")
                .args(["type", "--", text])
                .status()?;
            if status.success() {
                return Ok(());
            }
            anyhow::bail!("xdotool exited with status {}", status);
        }
        Some(LinuxTypingBackend::Wtype) => {
            let status = std::process::Command::new("wtype").arg(text).status()?;
            if status.success() {
                return Ok(());
            }
            anyhow::bail!("wtype exited with status {}", status);
        }
        Some(LinuxTypingBackend::Ydotool) => {
            let status = std::process::Command::new("ydotool")
                .args(["type", text])
                .status()?;
            if status.success() {
                return Ok(());
            }
            anyhow::bail!("ydotool exited with status {}", status);
        }
        None => {
            let session_type =
                std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());
            anyhow::bail!(
                "No typing tool available for {} session. \
                 Install xdotool (X11), wtype (Wayland), or ydotool (either).",
                session_type
            );
        }
    }
}
