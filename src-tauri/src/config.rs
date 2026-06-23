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
    /// Default AUS (TJ-Entscheidung 2026-06-23). Nutzer-Präferenz → bleibt über Logout erhalten
    /// (NICHT in clear_account zurückgesetzt).
    #[serde(default)]
    pub forge_overlay_enabled: bool,
    /// Trace-Task-Mining: ob die Hintergrund-Erfassung läuft. PERSISTIERT → überlebt
    /// App-Schließen UND Reboot und wird beim Start automatisch wieder gearmt (lib.rs setup).
    /// So kann der Nutzer die Aufzeichnung NICHT durch Fenster-Schließen aushebeln — der Sinn
    /// des Task-Minings bleibt erhalten. Aktivierung = bewusster Schritt (expliziter Toggle =
    /// Opt-in); einmal an → bleibt an über App-Schließen/Reboot. Wird beim LOGOUT gelöscht
    /// (clear_account) + Engine gestoppt → kein Recorder ohne angemeldeten, einwilligenden
    /// Account (Security-Review H1). „Beim Kunden immer an" kommt über den Consent-Flow, nicht hier.
    #[serde(default)]
    pub trace_capture_enabled: bool,
}

// Manuelles Default — die persistierten Defaults explizit halten (forge_overlay_enabled AUS,
// trace_capture_enabled AUS; letzteres wird bei Kunden-Pairing automatisch aktiviert, s. auth.rs).
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
            forge_overlay_enabled: false,
            trace_capture_enabled: false,
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
        // Consent-Widerruf beim Logout: Erfassung NICHT über einen Account-/Owner-Wechsel
        // hinweg weiterlaufen lassen (Security-Review H1). Engine wird in account_logout gestoppt.
        self.trace_capture_enabled = false;
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
