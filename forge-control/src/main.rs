//! forge-control CLI — von der Bridge per Subprozess aufgerufen (hinter Session-Approval).
//!   forge-control capture [--out <datei>]   → base64-PNG auf stdout (oder Datei)
//!   forge-control act                        → liest Aktions-JSON von stdin und führt es aus
//!   forge-control stream [--fps N] [--quality Q] [--max-width W]
//!                                            → kontinuierlicher JPEG-Live-Stream auf stdout (L3)

use clap::{Parser, Subcommand};
use std::io::Read;

#[derive(Parser)]
#[command(name = "forge-control", about = "Augen + Hände für Forge-Remote-Work")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Screenshot des primären Monitors. Default: base64-PNG auf stdout; mit --out als Datei.
    Capture {
        #[arg(long)]
        out: Option<String>,
    },
    /// Aktion ausführen; JSON von stdin, z. B. {"type":"click","x":10,"y":20}.
    Act,
    /// Kontinuierlicher Live-Stream des primären Monitors auf stdout (Forge L3).
    ///
    /// Binär-Protokoll, length-prefixed:
    ///   1. Header-Zeile (JSON + '\n'): {"v":1,"w":<orig_w>,"h":<orig_h>,"fps":<fps>,"format":"jpeg"}
    ///      — `w`/`h` = ORIGINAL-Capture-Auflösung (Klick-Koordinatenraum, NICHT die skalierte).
    ///   2. dann je Frame: 4 Byte big-endian Länge (u32) + JPEG-Bytes.
    ///
    /// Läuft bis stdin-EOF (Bridge schließt die Pipe) oder Prozess-Kill. Capture-Fehler
    /// überspringen das Frame statt zu crashen.
    Stream {
        /// Bilder pro Sekunde (1..=30).
        #[arg(long, default_value_t = 8)]
        fps: u8,
        /// JPEG-Qualität (1..=100). Niedriger = weniger Bandbreite.
        #[arg(long, default_value_t = 55)]
        quality: u8,
        /// Maximale Frame-Breite in px; größere Captures werden runterskaliert (0 = keine Grenze).
        #[arg(long, default_value_t = 1280)]
        max_width: u32,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().cmd {
        Cmd::Capture { out } => {
            let png = forge_control::capture_primary_png()?;
            match out {
                Some(path) => std::fs::write(&path, &png)?,
                None => {
                    use base64::Engine;
                    println!("{}", base64::engine::general_purpose::STANDARD.encode(&png));
                }
            }
        }
        Cmd::Act => {
            // Eingabe begrenzen (DoS-Schutz: kein unbegrenztes stdin). 64 KB > jede legitime Aktion.
            let mut input = String::new();
            std::io::stdin().take(64 * 1024).read_to_string(&mut input)?;
            let action: forge_control::Action = serde_json::from_str(input.trim())?;
            forge_control::perform(&action)?;
        }
        Cmd::Stream {
            fps,
            quality,
            max_width,
        } => {
            forge_control::run_stream(fps, quality, max_width)?;
        }
    }
    Ok(())
}
