# Sonar (sonar-tauri)

**Sonar** — der Agent auf der Maschine. Eine Tauri-2-App (Rust + WebView), die mehrere
On-Site-Werkzeuge unter einem Dach + einem Installer vereint. Jedes Tool hat einen eigenen
„Space":

- **Forge** — Remote-Support (läuft auf der gebündelten Subunit Bridge).
- **Trace** — Task-Mining (eigene Capture-Engine; folgt).
- **Home** — Überblick + Account.

Login über **auth.subunit.ai** (OAuth-Loopback). Account-gesteuerter Zugriff:
subunit-Accounts = voller Zugriff, Kunden = explizite Freigabe (Consent-Gate der Bridge).

## Architektur
- **sonar-tauri** (dies hier) — die App + das wiederverwendete Tauri-Fundament (Sidecar-Supervisor,
  Supply-Chain-Verify, Consent-Plumbing, Updater, Tray/Autostart). Baut die Installer, bündelt die Sidecars.
- **bridge-tauri** — der gehärtete Bun-Bridge-Daemon, als kompilierter Sidecar gebündelt (eigenes Repo, eigene Versionen).
- **trace-tauri** — die Task-Mining-Capture-Engine, als Sidecar gebündelt (eigenes Repo, eigene Versionen; folgt).

Sidecars werden in CI **einmal auf Linux** gebaut (Bun cross-compile inkl. Windows-/macOS-Targets),
per SHA-256-Manifest gepinnt und als Artifact an die Plattform-Builds verteilt. Die App verifiziert
den Sidecar-Hash beim Start (`supply_chain.rs`, fail-closed).

## Build
```bash
# Frontend
bun install
bun run build

# Sidecar(s) lokal holen/bauen (braucht bun + bridge-Source unter ../subunit-bridge)
bash scripts/fetch-sidecars.sh

# App
bun run tauri build
```

## Release
Tag `v*` pushen → GitHub Actions baut signierte Installer für **Windows x64, Windows ARM64,
Linux (deb) und macOS (x64 + arm64, dmg)** + den Updater-Manifest (`latest.json`) als Draft-Release.
Code-Signing (Windows Authenticode / Apple Notarisierung) ist cert-gated verdrahtet.

## Plattformen
Windows x64 · Windows ARM64 · Linux · macOS (Intel + Apple Silicon)
