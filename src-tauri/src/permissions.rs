//! OS-Permission-Onboarding (1.2): die Laufzeit-Grants, die Forge auf macOS braucht —
//! Screen-Recording (xcap-Capture) + Accessibility (enigo-Input). macOS bietet dafür
//! KEINE Info.plist-Usage-Strings (anders als Mic/Cam) → der Grant läuft rein über TCC /
//! System Settings, deshalb das geführte Onboarding hier.
//!
//! Windows/Linux haben keine vergleichbare Laufzeit-Sperre für Screen-Capture/Input
//! (UAC/Defender sind Installations-, keine Laufzeit-Themen) → needs_grants = false.

use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct PermissionStatus {
    pub screen_recording: bool,
    pub accessibility: bool,
    /// Braucht diese Plattform überhaupt Laufzeit-Grants? Nur macOS → true.
    pub needs_grants: bool,
    pub os: &'static str,
}

// ----------------------------- macOS -----------------------------
#[cfg(target_os = "macos")]
mod platform {
    use super::PermissionStatus;

    // CoreFoundation Boolean = unsigned char.
    type Boolean = u8;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        // true, wenn die App Screen-Recording-Rechte hat (kein Prompt).
        fn CGPreflightScreenCaptureAccess() -> bool;
        // Fügt die App in die Screen-Recording-Liste ein und promptet einmalig.
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        // true, wenn die App als Accessibility-Client getrusted ist (enigo-Input).
        fn AXIsProcessTrusted() -> Boolean;
    }

    pub fn check() -> PermissionStatus {
        let screen_recording = unsafe { CGPreflightScreenCaptureAccess() };
        let accessibility = unsafe { AXIsProcessTrusted() != 0 };
        PermissionStatus { screen_recording, accessibility, needs_grants: true, os: "macos" }
    }

    pub fn request_screen() -> bool {
        unsafe { CGRequestScreenCaptureAccess() }
    }

    pub fn settings_url(which: &str) -> Option<&'static str> {
        match which {
            "screen_recording" => {
                Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            }
            "accessibility" => {
                Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            }
            _ => None,
        }
    }
}

// -------------------------- Windows / Linux ----------------------
#[cfg(not(target_os = "macos"))]
mod platform {
    use super::PermissionStatus;

    pub fn check() -> PermissionStatus {
        PermissionStatus {
            screen_recording: true,
            accessibility: true,
            needs_grants: false,
            os: if cfg!(target_os = "windows") { "windows" } else { "linux" },
        }
    }
    pub fn request_screen() -> bool {
        true
    }
    pub fn settings_url(_which: &str) -> Option<&'static str> {
        None
    }
}

// ----------------------------- shared ----------------------------
#[cfg(target_os = "macos")]
fn open_url(url: &str) -> std::io::Result<()> {
    std::process::Command::new("open").arg(url).spawn().map(|_| ())
}
#[cfg(not(target_os = "macos"))]
fn open_url(_url: &str) -> std::io::Result<()> {
    Ok(())
}

// --------------------------- Tauri commands ----------------------
/// Aktueller Grant-Status (vom Onboarding-Wizard gepollt, bis alles grün ist).
#[tauri::command]
pub fn permissions_check() -> PermissionStatus {
    platform::check()
}

/// Löst (macOS) den einmaligen Screen-Recording-Systemprompt aus. Danach muss der
/// Nutzer den Toggle ggf. noch in System Settings setzen — daher zusätzlich open_settings.
#[tauri::command]
pub fn permissions_request_screen() -> bool {
    platform::request_screen()
}

/// Öffnet die passende System-Settings-Datenschutz-Sektion (Deeplink).
#[tauri::command]
pub fn permissions_open_settings(which: String) -> Result<(), String> {
    let url = platform::settings_url(&which)
        .ok_or_else(|| format!("keine Settings-URL für: {which}"))?;
    open_url(url).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permissions_check() {
        let status = permissions_check();

        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(
                status,
                PermissionStatus {
                    screen_recording: true,
                    accessibility: true,
                    needs_grants: false,
                    os: if cfg!(target_os = "windows") { "windows" } else { "linux" },
                }
            );
        }

        #[cfg(target_os = "macos")]
        {
            // On macOS, the actual boolean values might vary based on whether the test
            // is run in an environment with permissions granted or not.
            // But we can check that needs_grants is true and os is "macos"
            assert_eq!(status.needs_grants, true);
            assert_eq!(status.os, "macos");
        }
    }
}
