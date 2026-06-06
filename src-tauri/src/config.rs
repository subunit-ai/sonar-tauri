//! Schlanker Account-/Token-Store für Sonar. Hält die auth.subunit.ai-Tokens
//! + den signierten `op`-Claim (treibt später die Access-Policy: subunit = voll,
//! Kunde = Consent). Persistenz als JSON unter dem OS-Config-Dir (`<config>/sonar/config.json`).
//!
//! Hinweis (Hardening-Follow-up): Tokens liegen — wie bei Echo — als Klartext-JSON
//! im user-scoped Config-Dir. OS-Keychain ist ein späterer Härtungsschritt.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
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
    /// Operator-Claim (`op`) aus dem JWT — NUR UI-Label. Wird clientseitig OHNE
    /// Signatur-Verify dekodiert → NIEMALS als Sicherheitsgrenze nutzen. Die echte
    /// Access-Policy (subunit=voll, Kunde=Consent) erzwingt die Bridge serverseitig
    /// gegen JWKS (P2) — nie dieser Wert. (Codex-Finding #2)
    #[serde(default)]
    pub account_is_operator: bool,
    /// Forge-Einstellung: ob das „u1 arbeitet"-Overlay während Remote-Zugriff angezeigt wird.
    /// Default TRUE (Transparenz beim Fernzugriff). Nutzer-Präferenz → bleibt über Logout erhalten
    /// (NICHT in clear_account zurückgesetzt).
    #[serde(default = "default_true")]
    pub forge_overlay_enabled: bool,
}

fn default_true() -> bool {
    true
}

// Manuelles Default (statt derive) — der derive würde `forge_overlay_enabled` auf `false` setzen;
// load() nutzt unwrap_or_default() für Neu-Installs → muss TRUE sein (Overlay default an).
impl Default for Config {
    fn default() -> Self {
        Self {
            subunit_access_token: String::new(),
            subunit_refresh_token: String::new(),
            subunit_token_expires_in: 0,
            subunit_token_issued_at: 0.0,
            subunit_workspace_id: String::new(),
            account_email: String::new(),
            account_is_operator: false,
            forge_overlay_enabled: true,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        config_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Atomar schreiben (temp + rename) und auf Unix mit 0600 absichern, damit der
    /// Refresh-Token nicht von anderen Usern lesbar ist (Codex-Finding #3).
    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path().ok_or_else(|| anyhow::anyhow!("no config dir available"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &data)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &path)?;
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
