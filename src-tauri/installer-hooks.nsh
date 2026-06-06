; Sonar NSIS-Installer-Hooks
; Zweck: den Bridge-Sidecar (subunit-bridge.exe) beenden, BEVOR der Installer Dateien schreibt.
; Tauris NSIS-Template beendet zwar die Haupt-App (Sonar.exe), kennt aber den separat laufenden
; Sidecar nicht — dessen .exe bleibt gesperrt → "Error opening file for writing: ...subunit-bridge.exe".
; Greift für JEDEN Install-Pfad (manueller Setup-Download UND der Auto-Updater, der denselben
; NSIS-Schritt fährt). nsExec::Exec läuft ohne sichtbares Konsolenfenster.

!macro NSIS_HOOK_PREINSTALL
  ; taskkill mit ABSOLUTEM Pfad ($SYSDIR = C:\Windows\System32) aufrufen, nie nur "taskkill" —
  ; sonst könnte eine untergeschobene taskkill.exe früher im %PATH%/CWD beim Install ausgeführt
  ; werden (PATH-Hijack). (Gemini-Review P0)
  ; Erster Kill: laufende/verwaiste Bridge-Sidecars dieser Session beenden.
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM subunit-bridge.exe /T'
  Pop $0
  ; Kurze Pause + zweiter Kill: Tauris Template hat Sonar.exe zu diesem Zeitpunkt bereits beendet,
  ; falls aber ein Supervisor-Poll-Zyklus den Sidecar noch einmal respawnt haben sollte, fängt
  ; ihn dieser zweite Durchgang — so ist die .exe beim Kopieren garantiert frei.
  Sleep 600
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM subunit-bridge.exe /T'
  Pop $0
!macroend
