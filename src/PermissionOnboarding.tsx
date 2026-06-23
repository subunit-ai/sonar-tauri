import { useCallback, useEffect, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";

type PermissionStatus = {
  screen_recording: boolean;
  accessibility: boolean;
  needs_grants: boolean;
  os: string;
};

/**
 * Onboarding-Wizard für die OS-Berechtigungen, die Forge braucht (1.2).
 * macOS: Screen-Recording (xcap) + Accessibility (enigo) — beides nur über TCC /
 * System Settings setzbar (keine Info.plist-Usage-Strings). Wir prüfen den Status,
 * öffnen die passende Settings-Sektion und pollen, bis alles grün ist.
 * Windows/Linux: needs_grants=false → die Karte rendert nichts.
 */
export function PermissionOnboarding() {
  const [status, setStatus] = useState<PermissionStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await invoke<PermissionStatus>("permissions_check"));
    } catch {
      /* Bridge/Command nicht bereit — beim nächsten Poll erneut */
    }
  }, []);

  useEffect(() => {
    refresh();
    const id = window.setInterval(refresh, 2000);
    return () => window.clearInterval(id);
  }, [refresh]);

  // Plattform braucht keine Grants ODER alles erteilt → Karte verschwindet.
  if (!status || !status.needs_grants) return null;
  if (status.screen_recording && status.accessibility) return null;

  const grantScreen = async () => {
    setBusy("screen_recording");
    try {
      // einmaliger System-Prompt + Settings-Deeplink (Toggle final in Settings)
      await invoke("permissions_request_screen");
      await invoke("permissions_open_settings", { which: "screen_recording" });
    } catch {
      /* ignored */
    } finally {
      setBusy(null);
      refresh();
    }
  };
  const grantAccessibility = async () => {
    setBusy("accessibility");
    try {
      await invoke("permissions_open_settings", { which: "accessibility" });
    } catch {
      /* ignored */
    } finally {
      setBusy(null);
      refresh();
    }
  };

  return (
    <div style={styles.card}>
      <div style={styles.header}>
        <span style={styles.title}>Berechtigungen einrichten</span>
        <span style={styles.badge}>Einrichtung</span>
      </div>
      <p style={styles.lead}>
        Damit Forge deinen Bildschirm sehen und Eingaben ausführen kann, braucht Sonar zwei
        macOS-Freigaben. Klick auf „Erlauben“, setz den Schalter in den Systemeinstellungen —
        der Haken erscheint hier automatisch.
      </p>
      <PermRow
        label="Bildschirmaufnahme"
        hint="Forge sieht den Bildschirm (Support, Live-Stream)"
        granted={status.screen_recording}
        busy={busy === "screen_recording"}
        onGrant={grantScreen}
      />
      <PermRow
        label="Bedienungshilfen"
        hint="Forge steuert Maus & Tastatur"
        granted={status.accessibility}
        busy={busy === "accessibility"}
        onGrant={grantAccessibility}
      />
    </div>
  );
}

function PermRow({
  label,
  hint,
  granted,
  busy,
  onGrant,
}: {
  label: string;
  hint: string;
  granted: boolean;
  busy: boolean;
  onGrant: () => void;
}) {
  return (
    <div style={styles.row}>
      <div style={styles.rowText}>
        <span style={styles.rowLabel}>{label}</span>
        <span style={styles.rowHint}>{hint}</span>
      </div>
      {granted ? (
        <span style={styles.grantedBadge}>✓ Erteilt</span>
      ) : (
        <button type="button" style={{ ...styles.grantButton, opacity: busy ? 0.6 : 1 }} disabled={busy} onClick={onGrant}>
          {busy ? "Öffne…" : "Erlauben"}
        </button>
      )}
    </div>
  );
}

const styles: Record<string, CSSProperties> = {
  card: {
    border: "1px solid rgba(6, 182, 212, 0.28)",
    background: "linear-gradient(180deg, rgba(6,182,212,0.06), rgba(6,182,212,0.02))",
    borderRadius: 14,
    padding: "18px 20px",
    marginBottom: 18,
    display: "flex",
    flexDirection: "column",
    gap: 12,
  },
  header: { display: "flex", alignItems: "center", justifyContent: "space-between" },
  title: { fontSize: 15, fontWeight: 650, color: "#0f172a" },
  badge: {
    fontSize: 11,
    fontWeight: 600,
    color: "#075d6d",
    background: "rgba(6, 182, 212, 0.13)",
    borderRadius: 999,
    padding: "3px 10px",
  },
  lead: { fontSize: 13, lineHeight: 1.5, color: "#475569", margin: 0 },
  row: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: 12,
    padding: "10px 0",
    borderTop: "1px solid rgba(15, 23, 42, 0.06)",
  },
  rowText: { display: "flex", flexDirection: "column", gap: 2 },
  rowLabel: { fontSize: 14, fontWeight: 600, color: "#0f172a" },
  rowHint: { fontSize: 12, color: "#64748b" },
  grantedBadge: { fontSize: 13, fontWeight: 600, color: "#047857" },
  grantButton: {
    fontSize: 13,
    fontWeight: 600,
    color: "#fff",
    background: "#06b6d4",
    border: "none",
    borderRadius: 9,
    padding: "8px 16px",
    cursor: "pointer",
  },
};
