//! Schlanker Account-/Token-Store für Sonar. Hält die auth.subunit.ai-Tokens
//! + den signierten `op`-Claim (treibt später die Access-Policy: subunit = voll,
//! Kunde = Consent). Persistenz als JSON unter dem OS-Config-Dir (`<config>/sonar/config.json`).
//!
//! Hinweis (Hardening-Follow-up): Tokens liegen — wie bei Echo — als Klartext-JSON
//! im user-scoped Config-Dir. OS-Keychain ist ein späterer Härtungsschritt.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub subunit_access_token: String,
    #[serde(default)]
    pub subunit_refresh_token: String,
    #[serde(default)]
    pub subunit_token_expires_in: i32,
    #[serde(default)]
    pub subunit_token_issued_at: f64,
    #[serde(default)]
    pub subunit_workspace_id: String,
    #[serde(default)]
    pub account_email: String,
    /// Signierter Operator-Claim (`op`) aus dem JWT — NUR aus dem Token gelesen,
    /// nie lokal gesetzt. Treibt die Access-Policy in der Bridge (P2).
    #[serde(default)]
    pub account_is_operator: bool,
}

impl Config {
    pub fn load() -> Self {
        config_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path().ok_or_else(|| anyhow::anyhow!("no config dir available"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn is_logged_in(&self) -> bool {
        !self.subunit_access_token.is_empty() || !self.subunit_refresh_token.is_empty()
    }

    pub fn clear_account(&mut self) {
        self.subunit_access_token.clear();
        self.subunit_refresh_token.clear();
        self.subunit_token_expires_in = 0;
        self.subunit_token_issued_at = 0.0;
        self.subunit_workspace_id.clear();
        self.account_email.clear();
        self.account_is_operator = false;
    }
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("sonar").join("config.json"))
}

/// App-weiter Zustand: der Config-/Token-Store hinter einem Mutex.
pub struct AppState {
    pub config: Mutex<Config>,
}

impl AppState {
    pub fn load() -> Self {
        Self {
            config: Mutex::new(Config::load()),
        }
    }
}
