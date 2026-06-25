//! forge-control — „Augen + Hände" für Forge-Remote-Work auf dem Kunden-PC.
//!
//! Schlankes Rust-CLI: `capture` liefert einen Screenshot (PNG), `act` führt eine UI-Aktion
//! aus (Maus/Tastatur). Cross-platform via `xcap` (Capture) + `enigo` (Input). Wird von der
//! Bridge per Subprozess HINTER dem Session-Approval/Consent angesteuert — selbst hat es
//! KEINE Auth/Policy (das ist bewusst die Aufgabe der Bridge).
//!
//! Das `Action`-Enum ist der Vertrag, den u1-Loop, Bridge und dieses CLI teilen.

use serde::{Deserialize, Serialize};

/// Maustaste.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

/// Eine Steuer-Aktion. JSON via `type`-Tag, z. B. `{"type":"click","x":100,"y":200,"button":"left"}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    MouseMove {
        x: i32,
        y: i32,
    },
    Click {
        x: i32,
        y: i32,
        #[serde(default)]
        button: MouseButton,
    },
    DoubleClick {
        x: i32,
        y: i32,
    },
    Type {
        text: String,
    },
    /// Tastenkombination, z. B. "ctrl+c", "ctrl+shift+s", "return", "tab".
    Key {
        combo: String,
    },
    Scroll {
        #[serde(default)]
        dx: i32,
        #[serde(default)]
        dy: i32,
    },
}

// =====================================================================
// Capture (xcap)
// =====================================================================

/// Screenshot des primären Monitors als PNG-Bytes.
pub fn capture_primary_png() -> Result<Vec<u8>, String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    // Erster Monitor = üblicherweise der primäre. (Monitor-Auswahl später verfeinerbar.)
    let monitor = monitors
        .into_iter()
        .next()
        .ok_or_else(|| "kein Monitor gefunden".to_string())?;
    let image = monitor.capture_image().map_err(|e| e.to_string())?;
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

// =====================================================================
// Live-Stream (Forge L3) — kontinuierlicher JPEG-Frame-Strom auf stdout
// =====================================================================

/// Ein kodiertes Stream-Frame: JPEG-Bytes + die ORIGINALE Capture-Auflösung. `width`/`height`
/// sind bewusst die Original-Maße (NICHT die ggf. herunterskalierten), denn die Operator-Konsole
/// mappt Klicks anhand dieser Maße auf physische Bildschirm-Pixel — beim Downscaling dürfen die
/// Klick-Koordinaten nicht mitschrumpfen, sonst landet der Klick falsch.
pub struct CaptureFrame {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Kodiert ein bereits gecapturetes RGBA-Bild als JPEG-Frame: optional auf `max_width`
/// herunterskaliert (Seitenverhältnis erhalten), `quality` 1..=100. Reiner Bild→Bytes-Pfad
/// ohne Capture/IO → ideal unit-testbar (kein Display nötig). Gibt die ORIGINAL-Maße zurück.
pub fn encode_frame_jpeg(
    image: image::RgbaImage,
    quality: u8,
    max_width: u32,
) -> Result<CaptureFrame, String> {
    let (orig_w, orig_h) = (image.width(), image.height());
    let mut dynimg = image::DynamicImage::ImageRgba8(image);
    if max_width > 0 && orig_w > max_width {
        let new_h = ((orig_h as u64 * max_width as u64) / orig_w as u64).max(1) as u32;
        dynimg = dynimg.resize_exact(max_width, new_h, image::imageops::FilterType::Triangle);
    }
    // JPEG hat keinen Alpha-Kanal → nach RGB konvertieren.
    let rgb = dynimg.to_rgb8();
    let mut buf = std::io::Cursor::new(Vec::new());
    let q = quality.clamp(1, 100);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, q)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    Ok(CaptureFrame {
        jpeg: buf.into_inner(),
        width: orig_w,
        height: orig_h,
    })
}

/// Ein einzelnes Stream-Frame: primären Monitor capturen → `encode_frame_jpeg`.
pub fn capture_primary_jpeg(quality: u8, max_width: u32) -> Result<CaptureFrame, String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let monitor = monitors
        .into_iter()
        .next()
        .ok_or_else(|| "kein Monitor gefunden".to_string())?;
    let image = monitor.capture_image().map_err(|e| e.to_string())?;
    encode_frame_jpeg(image, quality, max_width)
}

/// Schreibt ein Frame ins length-prefixed Protokoll: 4 Byte big-endian Länge (u32) + JPEG-Bytes.
pub fn write_frame(out: &mut impl std::io::Write, jpeg: &[u8]) -> std::io::Result<()> {
    out.write_all(&(jpeg.len() as u32).to_be_bytes())?;
    out.write_all(jpeg)
}

/// Streamt den primären Monitor als JPEG-Frames auf stdout (Protokoll s. `main.rs::Cmd::Stream`).
///
/// Beendet sauber, sobald die Bridge die Pipe schließt: ein Hintergrund-Thread liest stdin und
/// setzt bei EOF ein Stop-Flag; zusätzlich bricht ein stdout-Schreibfehler (Leser weg) die Schleife
/// ab. Capture-/Encode-Fehler überspringen nur das jeweilige Frame statt den Prozess zu beenden.
pub fn run_stream(fps: u8, quality: u8, max_width: u32) -> Result<(), String> {
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let fps = fps.clamp(1, 30);
    let frame_interval = Duration::from_millis(1000 / fps as u64);

    // stdin-EOF = Stop-Signal (die Bridge schließt die Pipe, wenn der Stream enden soll).
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut buf = [0u8; 256];
            loop {
                match std::io::stdin().read(&mut buf) {
                    Ok(0) | Err(_) => {
                        stop.store(true, Ordering::SeqCst);
                        break;
                    }
                    Ok(_) => {} // beliebige stdin-Daten ignorieren; nur EOF beendet
                }
            }
        });
    }

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    // Header zuerst — mit der ORIGINAL-Auflösung fürs Klick-Mapping (ein erstes Frame liefert sie).
    let first = capture_primary_jpeg(quality, max_width)?;
    let header = format!(
        "{{\"v\":1,\"w\":{},\"h\":{},\"fps\":{},\"format\":\"jpeg\"}}\n",
        first.width, first.height, fps
    );
    out.write_all(header.as_bytes()).map_err(|e| e.to_string())?;
    if write_frame(&mut out, &first.jpeg).is_err() || out.flush().is_err() {
        return Ok(()); // Leser schon weg
    }

    while !stop.load(Ordering::SeqCst) {
        let t0 = Instant::now();
        match capture_primary_jpeg(quality, max_width) {
            Ok(frame) => {
                if write_frame(&mut out, &frame.jpeg).is_err() || out.flush().is_err() {
                    break; // stdout zu → Bridge ist weg
                }
            }
            Err(e) => eprintln!("[forge-control] Frame übersprungen: {e}"),
        }
        if let Some(rest) = frame_interval.checked_sub(t0.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    Ok(())
}

// =====================================================================
// Input (enigo)
// =====================================================================

use enigo::{Axis, Button, Direction, Enigo, Key, Keyboard, Mouse, Settings};

/// Setzt den Cursor absolut auf (x, y).
///
/// **Windows:** via `SetCursorPos` statt enigos `move_mouse(Abs)`. enigos absoluter
/// `SendInput`-Move-Pfad wurde auf realer Kundenhardware (frostbyte\finn, 2026-06-08)
/// für JEDE Koordinate (auch 0,0) mit „not all input events were sent (… UIPI)" abgewiesen,
/// während Tastatur + Wheel (auch `SendInput`) durchgingen → kein Privileg-/UIPI-Problem,
/// sondern der absolute-Move-Pfad selbst. `SetCursorPos` ist ein anderer API-Pfad und
/// funktioniert dort. DPI-Awareness (s. `ensure_dpi_aware`) sorgt dafür, dass die
/// Koordinaten den physischen Capture-Pixeln entsprechen.
///
/// **Andere OS:** unverändert enigo (auf dem Server verifiziert).
#[cfg(windows)]
fn position_cursor(_enigo: &mut Enigo, x: i32, y: i32) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
    unsafe { SetCursorPos(x, y) }.map_err(|e| format!("SetCursorPos failed: {e}"))
}

/// Primärer-Monitor-Scale (physische Pixel / logische Punkte). 1.0 wenn unbekannt.
#[cfg(target_os = "macos")]
fn primary_scale() -> f32 {
    xcap::Monitor::all()
        .ok()
        .and_then(|ms| ms.into_iter().next())
        .and_then(|m| m.scale_factor().ok())
        .filter(|s| *s > 0.0)
        .unwrap_or(1.0)
}

/// **macOS:** xcap captured physische Retina-Pixel, aber macOS-CoreGraphics-Input (enigo)
/// erwartet logische Punkte → physisch/scale, sonst landet der Klick um den Scale-Faktor
/// daneben (auf Retina ~2×). Auf Linux/X11 stimmen physisch + logisch überein → keine Division.
#[cfg(target_os = "macos")]
fn position_cursor(enigo: &mut Enigo, x: i32, y: i32) -> Result<(), String> {
    use enigo::Coordinate;
    let scale = primary_scale();
    let lx = (x as f32 / scale).round() as i32;
    let ly = (y as f32 / scale).round() as i32;
    enigo.move_mouse(lx, ly, Coordinate::Abs).map_err(|e| e.to_string())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn position_cursor(enigo: &mut Enigo, x: i32, y: i32) -> Result<(), String> {
    use enigo::Coordinate;
    enigo.move_mouse(x, y, Coordinate::Abs).map_err(|e| e.to_string())
}

/// Macht den Prozess Per-Monitor-DPI-aware, damit Cursor-Koordinaten in physischen Pixeln
/// (= Capture-Pixeln) liegen statt skaliert. Idempotent: ist es schon gesetzt, ignorieren.
#[cfg(windows)]
fn ensure_dpi_aware() {
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

#[cfg(not(windows))]
fn ensure_dpi_aware() {}

/// Führt eine Aktion aus. Erstellt pro Aufruf eine frische Enigo-Instanz (CLI = ein Schuss).
pub fn perform(action: &Action) -> Result<(), String> {
    ensure_dpi_aware();
    // Defense-in-depth: Feld-Längen begrenzen. Der Server validiert auch, aber dieses CLI ist
    // eigenständig und darf sich nicht auf den Aufrufer verlassen.
    match action {
        Action::Type { text } if text.chars().count() > 10_000 => return Err("text too long".into()),
        Action::Key { combo } if combo.chars().count() > 256 => return Err("combo too long".into()),
        _ => {}
    }
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    match action {
        Action::MouseMove { x, y } => {
            position_cursor(&mut enigo, *x, *y)?;
        }
        Action::Click { x, y, button } => {
            position_cursor(&mut enigo, *x, *y)?;
            enigo
                .button(to_button(*button), Direction::Click)
                .map_err(|e| e.to_string())?;
        }
        Action::DoubleClick { x, y } => {
            position_cursor(&mut enigo, *x, *y)?;
            enigo
                .button(Button::Left, Direction::Click)
                .map_err(|e| e.to_string())?;
            enigo
                .button(Button::Left, Direction::Click)
                .map_err(|e| e.to_string())?;
        }
        Action::Type { text } => {
            enigo.text(text).map_err(|e| e.to_string())?;
        }
        Action::Key { combo } => press_combo(&mut enigo, combo)?,
        Action::Scroll { dx, dy } => {
            if *dx != 0 {
                enigo.scroll(*dx, Axis::Horizontal).map_err(|e| e.to_string())?;
            }
            if *dy != 0 {
                enigo.scroll(*dy, Axis::Vertical).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

fn to_button(b: MouseButton) -> Button {
    match b {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
    }
}

/// Zerlegt eine Tastenkombination "ctrl+shift+s" → (`["ctrl","shift"]`, `"s"`). Lowercase, getrimmt.
pub fn split_combo(combo: &str) -> Result<(Vec<String>, String), String> {
    let parts: Vec<String> = combo
        .split('+')
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    match parts.split_last() {
        Some((key, mods)) => Ok((mods.to_vec(), key.clone())),
        None => Err(format!("leere Tastenkombination: {combo:?}")),
    }
}

fn press_combo(enigo: &mut Enigo, combo: &str) -> Result<(), String> {
    let (mods, key_str) = split_combo(combo)?;
    let key = parse_key(&key_str)?;
    let mod_keys = mods
        .iter()
        .map(|m| parse_modifier(m))
        .collect::<Result<Vec<Key>, String>>()?;
    for m in &mod_keys {
        enigo.key(*m, Direction::Press).map_err(|e| e.to_string())?;
    }
    let result = enigo.key(key, Direction::Click).map_err(|e| e.to_string());
    // Modifier IMMER wieder lösen — auch wenn der Tastendruck fehlschlug.
    for m in mod_keys.iter().rev() {
        let _ = enigo.key(*m, Direction::Release);
    }
    result
}

fn parse_modifier(m: &str) -> Result<Key, String> {
    Ok(match m {
        "ctrl" | "control" => Key::Control,
        "alt" | "option" => Key::Alt,
        "shift" => Key::Shift,
        "cmd" | "meta" | "super" | "win" => Key::Meta,
        other => return Err(format!("unbekannter Modifier: {other:?}")),
    })
}

fn parse_key(k: &str) -> Result<Key, String> {
    let chars: Vec<char> = k.chars().collect();
    if chars.len() == 1 {
        return Ok(Key::Unicode(chars[0]));
    }
    Ok(match k {
        "return" | "enter" => Key::Return,
        "tab" => Key::Tab,
        "esc" | "escape" => Key::Escape,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        other => return Err(format!("unbekannte Taste: {other:?}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_json_roundtrip() {
        let a = Action::Click {
            x: 100,
            y: 200,
            button: MouseButton::Right,
        };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"type\":\"click\""));
        assert!(json.contains("\"button\":\"right\""));
        assert_eq!(serde_json::from_str::<Action>(&json).unwrap(), a);
    }

    #[test]
    fn click_defaults_to_left_button() {
        let a: Action = serde_json::from_str(r#"{"type":"click","x":1,"y":2}"#).unwrap();
        assert_eq!(
            a,
            Action::Click {
                x: 1,
                y: 2,
                button: MouseButton::Left
            }
        );
    }

    #[test]
    fn type_and_scroll_parse() {
        assert_eq!(
            serde_json::from_str::<Action>(r#"{"type":"type","text":"hallo"}"#).unwrap(),
            Action::Type {
                text: "hallo".into()
            }
        );
        assert_eq!(
            serde_json::from_str::<Action>(r#"{"type":"scroll","dy":-3}"#).unwrap(),
            Action::Scroll { dx: 0, dy: -3 }
        );
    }

    #[test]
    fn combo_splits_modifiers_and_key() {
        assert_eq!(
            split_combo("ctrl+shift+s").unwrap(),
            (vec!["ctrl".to_string(), "shift".to_string()], "s".to_string())
        );
        assert_eq!(split_combo("Return").unwrap(), (vec![], "return".to_string()));
        assert!(split_combo("").is_err());
    }

    #[test]
    fn encode_frame_jpeg_produces_valid_jpeg_and_reports_original_dims() {
        let img = image::RgbaImage::from_pixel(120, 80, image::Rgba([10, 120, 200, 255]));
        let frame = encode_frame_jpeg(img, 55, 0).unwrap();
        // JPEG-Magic SOI: FF D8 FF
        assert_eq!(&frame.jpeg[0..3], &[0xFF, 0xD8, 0xFF]);
        assert!(frame.jpeg.len() > 4);
        // Original-Maße werden gemeldet (Klick-Koordinatenraum).
        assert_eq!((frame.width, frame.height), (120, 80));
    }

    #[test]
    fn encode_frame_jpeg_downscales_but_reports_original_dims() {
        let img = image::RgbaImage::from_pixel(1000, 500, image::Rgba([0, 0, 0, 255]));
        let frame = encode_frame_jpeg(img, 40, 200).unwrap();
        // Auch herunterskaliert bleibt die GEMELDETE Auflösung die originale — sonst landen Klicks falsch.
        assert_eq!((frame.width, frame.height), (1000, 500));
        assert_eq!(&frame.jpeg[0..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn write_frame_prefixes_big_endian_length() {
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &[1, 2, 3, 4, 5]).unwrap();
        assert_eq!(&buf[0..4], &[0, 0, 0, 5]); // u32 big-endian
        assert_eq!(&buf[4..], &[1, 2, 3, 4, 5]);
    }
}
