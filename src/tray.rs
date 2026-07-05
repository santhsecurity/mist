//! System tray icon setup and helper to open paths in the platform file manager.

use log::warn;
use mist::icon;
use std::path::Path;
use std::process::Command;

/// Optional system tray state. Any part may be None if the tray backend is
/// unavailable; Mist continues to work normally without it.
pub struct TrayState {
    _icon: Option<tray_icon::TrayIcon>,
    pub open_config_id: Option<tray_icon::menu::MenuId>,
    pub open_logs_id: Option<tray_icon::menu::MenuId>,
    pub quit_id: Option<tray_icon::menu::MenuId>,
}

pub fn init_tray() -> TrayState {
    let Some((rgba, width, height)) = icon::app_icon_rgba() else {
        warn!("Failed to render tray icon");
        return TrayState {
            _icon: None,
            open_config_id: None,
            open_logs_id: None,
            quit_id: None,
        };
    };
    let Ok(icon) = tray_icon::Icon::from_rgba(rgba, width, height) else {
        warn!("Failed to create tray icon from RGBA");
        return TrayState {
            _icon: None,
            open_config_id: None,
            open_logs_id: None,
            quit_id: None,
        };
    };

    let menu = tray_icon::menu::Menu::new();
    let open_config = tray_icon::menu::MenuItem::new("Open config folder", true, None);
    let open_logs = tray_icon::menu::MenuItem::new("Open data folder", true, None);
    let quit = tray_icon::menu::MenuItem::new("Quit", true, None);
    let open_config_id = open_config.id().clone();
    let open_logs_id = open_logs.id().clone();
    let quit_id = quit.id().clone();

    let _ = menu.append(&open_config);
    let _ = menu.append(&open_logs);
    let _ = menu.append(&tray_icon::menu::PredefinedMenuItem::separator());
    let _ = menu.append(&quit);

    let icon = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_title("Mist")
        .with_tooltip("Mist dictation daemon")
        .build();

    if let Err(ref e) = icon {
        warn!("Failed to create tray icon: {}", e);
    }

    TrayState {
        _icon: icon.ok(),
        open_config_id: Some(open_config_id),
        open_logs_id: Some(open_logs_id),
        quit_id: Some(quit_id),
    }
}

/// Best-effort open a path in the system file manager.
pub fn open_path(path: &Path) -> std::io::Result<()> {
    let (cmd, arg) = if cfg!(target_os = "macos") {
        ("open", path.as_os_str())
    } else if cfg!(target_os = "windows") {
        ("explorer", path.as_os_str())
    } else {
        ("xdg-open", path.as_os_str())
    };
    Command::new(cmd).arg(arg).spawn()?;
    Ok(())
}
