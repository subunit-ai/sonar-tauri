use crate::{bridge_client::BridgeClient, bridge_client::BridgeHealth, supply_chain};
use serde::Serialize;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Runtime};
use tauri_plugin_shell::{
    process::{CommandChild, CommandEvent},
    ShellExt,
};
use tokio::time::{sleep, Duration};

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
            Ok(health) => self.set_online(health),
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

    async fn spawn_verified<R>(&self, app: &AppHandle<R>) -> Result<(), String>
    where
        R: Runtime,
    {
        if !self.should_spawn() {
            return Ok(());
        }

        let sidecar_path = supply_chain::verify_resolved_sidecar()?;
        let (mut rx, child) = app
            .shell()
            .sidecar(supply_chain::SIDECAR_NAME)
            .map_err(|error| {
                format!(
                    "failed to prepare bridge sidecar at {}: {error}",
                    sidecar_path.display()
                )
            })?
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
            if state.stopping {
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
        !state.stopping && state.child.is_none()
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
