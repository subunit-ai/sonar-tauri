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
// Input (enigo)
// =====================================================================

use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};

/// Führt eine Aktion aus. Erstellt pro Aufruf eine frische Enigo-Instanz (CLI = ein Schuss).
pub fn perform(action: &Action) -> Result<(), String> {
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
            enigo.move_mouse(*x, *y, Coordinate::Abs).map_err(|e| e.to_string())?;
        }
        Action::Click { x, y, button } => {
            enigo.move_mouse(*x, *y, Coordinate::Abs).map_err(|e| e.to_string())?;
            enigo
                .button(to_button(*button), Direction::Click)
                .map_err(|e| e.to_string())?;
        }
        Action::DoubleClick { x, y } => {
            enigo.move_mouse(*x, *y, Coordinate::Abs).map_err(|e| e.to_string())?;
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
}
