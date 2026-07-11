#!/usr/bin/env pwsh
# Windows-Authenticode: signiert den Tauri-NSIS-Installer + die App-EXE (cert-gated).
# Wird im Release-Workflow (release.yml) UND im Installer-Check (win-installer-check.yml)
# jeweils VOR dem Tauri-Build aufgerufen — nur auf Windows-Runnern.
#
# Mechanik: Tauri v2 ruft signtool.exe automatisch auf, sobald
# bundle.windows.certificateThumbprint gesetzt ist. Wir importieren das PFX-Cert
# (base64-Secret) in den CurrentUser-Store, lesen den Thumbprint und schreiben ihn in
# tauri.conf.json — nur im ephemeren CI-Checkout, nie committet.
#
# Secrets (subunit-ai/sonar-tauri -> Settings -> Secrets and variables -> Actions):
#   WINDOWS_CERTIFICATE           base64 des .pfx (OV- oder EV-Code-Signing-Zertifikat)
#   WINDOWS_CERTIFICATE_PASSWORD  Passwort des .pfx
# Ohne Secret => No-Op: der Build bleibt unsigniert (wie bisher), CI faellt NICHT.
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($env:WINDOWS_CERTIFICATE)) {
  Write-Host "win-authenticode: kein WINDOWS_CERTIFICATE gesetzt -> Installer bleibt unsigniert (SmartScreen-Warnung erwartet). Kein Fehler."
  exit 0
}

$tmp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
$pfx = Join-Path $tmp "sonar-authenticode.pfx"
[IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))

$pwPlain = if ($env:WINDOWS_CERTIFICATE_PASSWORD) { $env:WINDOWS_CERTIFICATE_PASSWORD } else { "" }
$pw = ConvertTo-SecureString -String $pwPlain -Force -AsPlainText
$cert = Import-PfxCertificate -FilePath $pfx -CertStoreLocation Cert:\CurrentUser\My -Password $pw
Remove-Item $pfx -Force
$thumb = $cert.Thumbprint
Write-Host "win-authenticode: Cert importiert, Thumbprint=$thumb"

$conf = "src-tauri/tauri.conf.json"
$json = Get-Content $conf -Raw | ConvertFrom-Json
if (-not $json.bundle.windows) {
  $json.bundle | Add-Member -NotePropertyName windows -NotePropertyValue ([pscustomobject]@{}) -Force
}
# certificateThumbprint = eigentlicher Signatur-Schalter; digest/timestamp sind schon in
# tauri.conf.json committet (harmlos ohne Thumbprint) -> nur setzen falls jemand sie entfernt.
$json.bundle.windows | Add-Member -NotePropertyName certificateThumbprint -NotePropertyValue $thumb -Force
if (-not $json.bundle.windows.digestAlgorithm) {
  $json.bundle.windows | Add-Member -NotePropertyName digestAlgorithm -NotePropertyValue "sha256" -Force
}
if (-not $json.bundle.windows.timestampUrl) {
  $json.bundle.windows | Add-Member -NotePropertyName timestampUrl -NotePropertyValue "http://timestamp.digicert.com" -Force
}
# -Depth hoch genug fuer die verschachtelte Config; UTF-8 OHNE BOM (serde_json vertraegt kein BOM).
$out = $json | ConvertTo-Json -Depth 64
[IO.File]::WriteAllText($conf, $out, (New-Object System.Text.UTF8Encoding $false))
Write-Host "win-authenticode: certificateThumbprint in $conf gesetzt -> Tauri signiert NSIS-Installer + App-EXE per signtool (RFC3161-Timestamp)."
