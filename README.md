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

### Windows-Signatur aktivieren (One-Click-Installer ohne SmartScreen-Warnung)
Ohne Code-Signing zeigt Windows beim ersten Start „Unbekannter Herausgeber" (SmartScreen) —
die groesste Kunden-Huerde. So schalten wir die Signatur scharf:

1. OV- oder EV-Code-Signing-Zertifikat als `.pfx` beschaffen (EV umgeht SmartScreen sofort;
   OV baut Reputation erst ueber Installs auf).
2. In `subunit-ai/sonar-tauri` → Settings → Secrets and variables → Actions anlegen:
   - `WINDOWS_CERTIFICATE` = base64 des `.pfx` (`base64 -w0 cert.pfx`)
   - `WINDOWS_CERTIFICATE_PASSWORD` = PFX-Passwort
3. Fertig — `scripts/win-authenticode-prep.ps1` importiert das Cert im Release-Build und setzt
   `certificateThumbprint`; Tauri signiert NSIS-Installer + App-EXE per signtool (RFC3161-Timestamp).
   Ohne die Secrets bleibt der Build unsigniert (wie bisher), CI faellt nicht.

Verifizierbar ohne Release: der Workflow **`win-installer-check.yml`** baut den Windows-x64-
NSIS-Installer aus einem PR / per `workflow_dispatch` und laedt ihn als Artifact hoch (kein Tag,
kein Release). Ist ein Cert gesetzt, prueft der Lauf die Authenticode-Signatur der Setup.exe.

## Plattformen
Windows x64 · Windows ARM64 · Linux · macOS (Intel + Apple Silicon)
