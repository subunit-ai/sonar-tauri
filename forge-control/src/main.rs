//! forge-control CLI — von der Bridge per Subprozess aufgerufen (hinter Session-Approval).
//!   forge-control capture [--out <datei>]   → base64-PNG auf stdout (oder Datei)
//!   forge-control act                        → liest Aktions-JSON von stdin und führt es aus

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
    }
    Ok(())
}
