//! Windows 11's Mica behind the window, as native apps have it. Windows 10 has no
//! Mica, and a see-through webview without it would show the desktop, so the
//! webview only turns transparent where Mica was applied.

use std::sync::OnceLock;

use tauri::WebviewWindow;

static MICA: OnceLock<bool> = OnceLock::new();

/// Mica arrived with Windows 11, build 22000.
#[cfg(windows)]
const FIRST_MICA_BUILD: u32 = 22000;

/// The Windows build number, from the registry.
#[cfg(windows)]
fn windows_build() -> Option<u32> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "/v",
            "CurrentBuildNumber",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    parse_build(&String::from_utf8_lossy(&output.stdout))
}

/// "    CurrentBuildNumber    REG_SZ    26200" → 26200.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_build(text: &str) -> Option<u32> {
    text.lines()
        .find(|line| line.contains("CurrentBuildNumber"))?
        .split_whitespace()
        .last()?
        .parse()
        .ok()
}

/// Puts Mica behind `window` on Windows 11 and makes the webview transparent.
pub fn apply(window: &WebviewWindow) {
    #[cfg(windows)]
    {
        use tauri::utils::config::WindowEffectsConfig;
        use tauri::window::{Color, Effect};

        let applied = windows_build().is_some_and(|build| build >= FIRST_MICA_BUILD)
            && window
                .set_effects(WindowEffectsConfig {
                    effects: vec![Effect::Mica],
                    ..Default::default()
                })
                .is_ok()
            && window.set_background_color(Some(Color(0, 0, 0, 0))).is_ok();
        let _ = MICA.set(applied);
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        let _ = MICA.set(false);
    }
}

/// "mica" when the window has Mica behind it, so the interface lets it through.
#[tauri::command]
pub fn backdrop() -> Option<&'static str> {
    MICA.get().copied().unwrap_or(false).then_some("mica")
}

/// Mica is tinted after the window's theme; it follows the app's light or dark.
#[tauri::command]
pub fn set_window_theme(dark: bool, window: WebviewWindow) {
    let theme = if dark {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    };
    let _ = window.set_theme(Some(theme));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_build_number() {
        let text = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\r\n    CurrentBuildNumber    REG_SZ    26200\r\n";
        assert_eq!(parse_build(text), Some(26200));
        assert_eq!(parse_build("nothing"), None);
    }
}
