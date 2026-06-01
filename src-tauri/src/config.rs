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
    /// Operator-Claim (`op`) aus dem JWT — NUR UI-Label. Wird clientseitig OHNE
    /// Signatur-Verify dekodiert → NIEMALS als Sicherheitsgrenze nutzen. Die echte
    /// Access-Policy (subunit=voll, Kunde=Consent) erzwingt die Bridge serverseitig
    /// gegen JWKS (P2) — nie dieser Wert. (Codex-Finding #2)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_logged_in() {
        let mut config = Config::default();

        // Initial state
        assert!(!config.is_logged_in());

        // Only access token
        config.subunit_access_token = "access_token".to_string();
        assert!(config.is_logged_in());

        // Only refresh token
        config.subunit_access_token.clear();
        config.subunit_refresh_token = "refresh_token".to_string();
        assert!(config.is_logged_in());

        // Both tokens
        config.subunit_access_token = "access_token".to_string();
        assert!(config.is_logged_in());
    }

    #[test]
    fn test_clear_account() {
        let mut config = Config {
            subunit_access_token: "access_token".to_string(),
            subunit_refresh_token: "refresh_token".to_string(),
            subunit_token_expires_in: 3600,
            subunit_token_issued_at: 1000.0,
            subunit_workspace_id: "workspace_1".to_string(),
            account_email: "test@example.com".to_string(),
            account_is_operator: true,
        };

        config.clear_account();

        assert!(config.subunit_access_token.is_empty());
        assert!(config.subunit_refresh_token.is_empty());
        assert_eq!(config.subunit_token_expires_in, 0);
        assert_eq!(config.subunit_token_issued_at, 0.0);
        assert!(config.subunit_workspace_id.is_empty());
        assert!(config.account_email.is_empty());
        assert!(!config.account_is_operator);
    }
}
