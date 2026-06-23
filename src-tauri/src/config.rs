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
    /// Trace-Consent-Status (Security-Review B3): "unset" | "granted" | "revoked".
    /// Die Hintergrund-Erfassung läuft AUSSCHLIESSLICH bei "granted" — gesetzt durch eine
    /// explizite, affirmative Einwilligung im Trace-Space (Transparenz-Screen). PERSISTIERT →
    /// Auto-Arm beim Launch (lib.rs setup) NUR bei "granted"; so überlebt eine einmal erteilte
    /// Einwilligung App-Schließen + Reboot („beim Kunden immer an"), ohne dass jemand sie durch
    /// Fenster-Schließen aushebelt. "revoked" (Stopp-Button/Logout) gewinnt IMMER und armt nie
    /// still neu — Re-Aktivierung erfordert frische Einwilligung. NIE an einem unverifizierten
    /// Claim auto-aktiviert (der entfernte op-Pfad war der Blocker).
    #[serde(default = "consent_unset")]
    pub trace_consent: String,
    /// Unix-Sekunden der Einwilligung (Audit-Provenance; wird als consent_at zu Nexus propagiert). 0 = keine.
    #[serde(default)]
    pub trace_consent_at: f64,
}

fn consent_unset() -> String {
    "unset".to_string()
}

// Manuelles Default — persistierte Defaults explizit (forge_overlay AUS, Trace-Consent "unset" →
// keine Erfassung ohne explizite Einwilligung).
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
            trace_consent: "unset".to_string(),
            trace_consent_at: 0.0,
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

    /// Läuft die Erfassung mit erteilter Einwilligung? Einziges Gate für Capture-Start/Auto-Arm.
    pub fn capture_consented(&self) -> bool {
        self.trace_consent == "granted"
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
        self.trace_consent = "revoked".to_string();
        self.trace_consent_at = 0.0;
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
