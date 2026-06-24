use crate::{bridge_client::BridgeClient, bridge_client::BridgeHealth, supply_chain};
use serde::Serialize;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};
use tokio::time::{sleep, Duration};

/// Ed25519-SPKI Public Key (PEM, mit literalen `\n`-Escapes — config.ts der Bridge
/// wandelt sie via `.replace(/\\n/g,"\n")` in echte Zeilenumbrüche zurück).
/// ÖFFENTLICHER Schlüssel → safe zum Einbetten; das private Gegenstück signiert die
/// Exec-Approvals serverseitig in subunit-api. Ohne diesen Key lehnt der gebündelte
/// Bridge-Sidecar JEDEN Remote-Exec mit "invalid approval signature" ab (er liest
/// EXEC_APPROVAL_PUBLIC_KEY aus process.env, das Sonar ihm beim Spawn mitgeben muss).
const EXEC_APPROVAL_PUBLIC_KEY_PEM: &str =
    "-----BEGIN PUBLIC KEY-----\\nMCowBQYDK2VwAyEA4D7u7gAC4BS4ES2vxctAvP97TDsoIvLdTASAD+oiPRM=\\n-----END PUBLIC KEY-----";

#[derive(Clone, Debug, Serialize)]
pub struct BridgeStatus {
    pub online: bool,
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paired: Option<bool>,
}

impl BridgeStatus {
    fn offline() -> Self {
        Self {
            online: false,
            version: None,
            paired: None,
        }
    }

    fn online(health: BridgeHealth) -> Self {
        Self {
            online: true,
            version: health.version,
            paired: health.paired,
        }
    }
}

#[derive(Clone)]
pub struct BridgeSupervisor {
    client: BridgeClient,
    inner: Arc<SupervisorInner>,
}

struct SupervisorInner {
    state: Mutex<BridgeRuntimeState>,
}

#[derive(Debug)]
struct BridgeRuntimeState {
    status: BridgeStatus,
    child: Option<CommandChild>,
    stopping: bool,
    /// Pausiert NUR für einen Update-Install (kein Respawn), ohne den Supervisor dauerhaft zu
    /// stoppen — schlägt der Install fehl, bringt resume_from_update() die Bridge zurück.
    updating: bool,
    /// Letzter Auto-Re-Pair-Versuch (Backoff: nicht öfter als alle ~12s, solange ungepairt).
    last_repair: Option<std::time::Instant>,
}

/// Killt verwaiste Bridge-Sidecar-Prozesse einer früheren Sonar-Instanz (nach Crash/Update-Restart
/// verliert die neue Instanz den Child-Handle → der alte Prozess bleibt im Task-Manager, belegt
/// Port :7842 und sperrt beim Install die .exe). Best-effort, plattformabhängig, ohne Konsolenfenster.
fn kill_orphan_sidecars() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let image = format!("{}.exe", supply_chain::SIDECAR_NAME);
        let mut cmd = std::process::Command::new("taskkill");
        cmd.arg("/F");
        // Nur Sidecars der EIGENEN Windows-Session killen — sonst beendet auf einem Multi-User-PC
        // Nutzer B beim Start die laufende Bridge von Nutzer A. (Gemini-Review P2)
        if let Ok(user) = std::env::var("USERNAME") {
            if !user.is_empty() {
                // taskkill matcht den Prozess-Owner als DOMAIN\User (bzw. COMPUTERNAME\User) — ohne
                // Domäne findet der Filter NICHTS, der Zombie überlebt und sperrt :7842 + die .exe.
                // USERDOMAIN voranstellen. (Gemini-Review P1)
                let domain = std::env::var("USERDOMAIN").unwrap_or_default();
                let user_query = if domain.is_empty() {
                    user
                } else {
                    format!("{domain}\\{user}")
                };
                cmd.args(["/FI", &format!("USERNAME eq {user_query}")]);
            }
        }
        let _ = cmd
            .args(["/IM", &image])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    #[cfg(not(windows))]
    {
        // -u <user>: nur eigene Prozesse (analog zum Windows-Filter, Multi-User-Sicherheit).
        let mut cmd = std::process::Command::new("pkill");
        if let Ok(user) = std::env::var("USER") {
            if !user.is_empty() {
                cmd.args(["-u", &user]);
            }
        }
        let _ = cmd.args(["-x", supply_chain::SIDECAR_NAME]).output();
    }
}

impl BridgeSupervisor {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            client: BridgeClient::new()?,
            inner: Arc::new(SupervisorInner {
                state: Mutex::new(BridgeRuntimeState {
                    status: BridgeStatus::offline(),
                    child: None,
                    stopping: false,
                    updating: false,
                    last_repair: None,
                }),
            }),
        })
    }

    pub fn start<R>(&self, app: AppHandle<R>)
    where
        R: Runtime,
    {
        let supervisor = self.clone();
        tauri::async_runtime::spawn(async move {
            // Verwaiste Sidecars einer früheren Instanz aufräumen, BEVOR wir health prüfen oder neu
            // spawnen — sonst antwortet ein Zombie auf :7842 und Sonar nutzt die alte (falsch/
            // ungepairte) Bridge, statt eine frische zu starten. (Finn-Bug: "pairt nicht" / .exe-Lock)
            kill_orphan_sidecars();
            supervisor.ensure_running(&app).await;

            loop {
                sleep(Duration::from_secs(5)).await;

                if supervisor.is_stopping() {
                    break;
                }

                supervisor.poll_once(&app).await;
            }
        });
    }

    pub fn status(&self) -> BridgeStatus {
        self.lock_state().status.clone()
    }

    pub fn stop(&self) {
        let child = {
            let mut state = self.lock_state();
            state.stopping = true;
            state.status = BridgeStatus::offline();
            state.child.take()
        };

        if let Some(child) = child {
            let pid = child.pid();
            if let Err(error) = child.kill() {
                eprintln!("failed to stop bridge sidecar pid {pid}: {error}");
            }
        }
    }

    /// Pausiert den Sidecar FÜR EIN UPDATE: killt den Prozess (gibt die .exe für den NSIS-Installer
    /// frei) und unterbindet ein Respawn durch den poll-Loop — stoppt den Supervisor aber NICHT
    /// dauerhaft. Schlägt der Install fehl, holt resume_from_update() die Bridge zurück. (Gemini-P0)
    pub fn pause_for_update(&self) {
        let child = {
            let mut state = self.lock_state();
            state.updating = true;
            state.status = BridgeStatus::offline();
            state.child.take()
        };
        if let Some(child) = child {
            let pid = child.pid();
            if let Err(error) = child.kill() {
                eprintln!("failed to pause bridge sidecar pid {pid} for update: {error}");
            }
        }
    }

    /// Hebt die Update-Pause auf → der poll-Loop spawnt die Bridge wieder (nach fehlgeschlagenem Install).
    pub fn resume_from_update(&self) {
        self.lock_state().updating = false;
    }

    async fn ensure_running<R>(&self, app: &AppHandle<R>)
    where
        R: Runtime,
    {
        match self.client.health().await {
            Ok(health) => self.set_online(health),
            Err(_) => {
                self.set_offline();
                if let Err(error) = self.spawn_verified(app).await {
                    eprintln!("{error}");
                }
            }
        }
    }

    async fn poll_once<R>(&self, app: &AppHandle<R>)
    where
        R: Runtime,
    {
        match self.client.health().await {
            Ok(health) => {
                let paired = health.paired;
                self.set_online(health);
                // AUTO-RE-PAIR: Bridge läuft, ist aber ungepairt (Login war zu früh / Token rotiert /
                // Sidecar neu gestartet). Statt auf einen manuellen „Koppeln"-Klick zu warten, koppeln
                // wir hier selbst — alle 5s, UI-unabhängig, mit Backoff. Macht das Pairing automatisch
                // + selbstheilend; der Button bleibt nur Override.
                if paired == Some(false) {
                    self.maybe_auto_repair(app).await;
                }
            }
            Err(_) => {
                self.set_offline();
                if self.should_spawn() {
                    if let Err(error) = self.spawn_verified(app).await {
                        eprintln!("{error}");
                    }
                }
            }
        }
    }

    /// Re-Adopt der lokalen Bridge, wenn sie online aber ungepairt ist — nur bei eingeloggtem
    /// Account (sonst kein Token) und höchstens alle ~12s (Backoff gegen /auth/adopt-Hämmern).
    async fn maybe_auto_repair<R>(&self, app: &AppHandle<R>)
    where
        R: Runtime,
    {
        let logged_in = app
            .try_state::<crate::config::AppState>()
            .map(|s| s.config.lock().is_logged_in())
            .unwrap_or(false);
        if !logged_in {
            return;
        }
        {
            let mut state = self.lock_state();
            let due = state
                .last_repair
                .map(|t| t.elapsed() >= Duration::from_secs(12))
                .unwrap_or(true);
            if !due {
                return;
            }
            state.last_repair = Some(std::time::Instant::now());
        } // Lock VOR dem await freigeben (std::sync::Mutex nie über await halten).
        match crate::auth::adopt_into_bridge(app).await {
            Ok(()) => eprintln!("[bridge] Auto-Re-Pair erfolgreich."),
            Err(error) => eprintln!("[bridge] Auto-Re-Pair fehlgeschlagen (Retry folgt): {error}"),
        }
    }

    async fn spawn_verified<R>(&self, app: &AppHandle<R>) -> Result<(), String>
    where
        R: Runtime,
    {
        if !self.should_spawn() {
            return Ok(());
        }

        let sidecar_path = supply_chain::verify_resolved_sidecar()?;

        // macOS: ein ad-hoc-signierter/heruntergeladener Build trägt die Quarantäne
        // (com.apple.quarantine) auch auf der GEBÜNDELTEN Sidecar-Binary → Gatekeeper killt den
        // Helper beim Spawn still → die Bridge kommt NIE online (Symptom: „verbindet…" für immer,
        // kein Auto-Pair, kein Koppeln-Button). Eine App darf ihre EIGENEN gebündelten Binaries
        // entquarantänisieren — das tun wir hier, VOR dem Spawn. (Stabiles Signing macht das später
        // unnötig; bis dahin der Selbstheil-Schritt, damit ad-hoc-Builds sich von alleine verbinden.)
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("/usr/bin/xattr")
                .args(["-dr", "com.apple.quarantine"])
                .arg(&sidecar_path)
                .output();
        }

        let (mut rx, child) = app
            .shell()
            .sidecar(supply_chain::SIDECAR_NAME)
            .map_err(|error| {
                format!(
                    "failed to prepare bridge sidecar at {}: {error}",
                    sidecar_path.display()
                )
            })?
            // Sidecar erbt das Parent-Env (env() ist additiv, kein env_clear) und bekommt
            // zusätzlich den Exec-Approval-Verifikationsschlüssel — sonst "invalid approval signature".
            .env("EXEC_APPROVAL_PUBLIC_KEY", EXEC_APPROVAL_PUBLIC_KEY_PEM)
            .spawn()
            .map_err(|error| {
                format!(
                    "failed to spawn bridge sidecar at {}: {error}",
                    sidecar_path.display()
                )
            })?;
        let pid = child.pid();

        {
            let mut state = self.lock_state();
            // stopping ODER updating: wurde während des (async) Spawns ein Stop bzw. eine Update-Pause
            // ausgelöst, den frisch gespawnten Sidecar sofort wieder killen — sonst liefe er trotz
            // Pause weiter und würde beim Update die .exe sperren. (Race-Schutz zur updating-Logik)
            if state.stopping || state.updating {
                drop(state);
                if let Err(error) = child.kill() {
                    return Err(format!("failed to stop bridge sidecar pid {pid}: {error}"));
                }
                return Ok(());
            }
            state.child = Some(child);
            state.status = BridgeStatus::offline();
        }

        let supervisor = self.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    CommandEvent::Terminated(_) => {
                        supervisor.clear_child(pid);
                        break;
                    }
                    CommandEvent::Error(error) => {
                        eprintln!("bridge sidecar pid {pid} reported an error: {error}");
                    }
                    CommandEvent::Stdout(_) | CommandEvent::Stderr(_) => {}
                    _ => {}
                }
            }
        });

        self.refresh_health_until_ready().await;
        Ok(())
    }

    async fn refresh_health_until_ready(&self) {
        for _ in 0..20 {
            if self.is_stopping() {
                return;
            }

            match self.client.health().await {
                Ok(health) => {
                    self.set_online(health);
                    return;
                }
                Err(_) => sleep(Duration::from_millis(250)).await,
            }
        }
    }

    fn clear_child(&self, pid: u32) {
        let mut state = self.lock_state();
        if state.child.as_ref().is_some_and(|child| child.pid() == pid) {
            state.child = None;
            state.status = BridgeStatus::offline();
        }
    }

    fn should_spawn(&self) -> bool {
        let state = self.lock_state();
        !state.stopping && !state.updating && state.child.is_none()
    }

    fn is_stopping(&self) -> bool {
        self.lock_state().stopping
    }

    fn set_online(&self, health: BridgeHealth) {
        self.lock_state().status = BridgeStatus::online(health);
    }

    fn set_offline(&self) {
        self.lock_state().status = BridgeStatus::offline();
    }

    fn lock_state(&self) -> MutexGuard<'_, BridgeRuntimeState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Drop for SupervisorInner {
    fn drop(&mut self) {
        let child = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.child.take());

        if let Some(child) = child {
            let pid = child.pid();
            if let Err(error) = child.kill() {
                eprintln!("failed to stop bridge sidecar pid {pid}: {error}");
            }
        }
    }
}
