import { isEqual } from "./utils";
import { useCallback, useEffect, useState, memo, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SonarLogo } from "./SonarLogo";

type TraceStatus = {
  capturing: boolean;
  total_events: number;
  last_event_at: number | null;
};

type ActivityEvent = {
  id: number;
  app_name: string;
  window_title: string;
  started_at: number;
  ended_at: number;
  duration_secs: number;
};

type AppUsage = {
  app_name: string;
  total_secs: number;
  episode_count: number;
};

type TraceConsent = {
  consent: "unset" | "granted" | "revoked";
  consent_at: number; // Unix-Sekunden (0 = keine)
};

// Transparenz: exakt was erfasst wird (MINING-PIPELINE §2) — und was NICHT.
const CAPTURED = [
  "Welche App/Programm im Vordergrund ist",
  "Fenstertitel (lokal maskiert, sensible Titel werden geschwärzt)",
  "Bedientes UI-Element + Aktionstyp (Klick, Eingabe, Kopieren, Wechsel)",
  "Aktive Web-Domain (nur Domain, NIE volle Adresse/Inhalt)",
  "Eingabe-LÄNGE (nur die Zeichenanzahl, NIE der getippte Text)",
  "Kopieren→Einfügen-Brücken (als Prüfsumme, NIE der Inhalt)",
];
const NOT_CAPTURED = [
  "Tastatureingaben im Klartext",
  "Inhalte der Zwischenablage",
  "Screenshots / Bildschirminhalt",
  "Mikrofon / Kamera",
  "Netzwerk-/Datei-Inhalte",
  "Passwörter, IBANs, sensible Felder (lokal geschwärzt)",
];

const POLL_MS = 4000;

function fmtDuration(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  return `${(secs / 3600).toFixed(1)}h`;
}

function fmtClock(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export const TraceSpace = memo(function TraceSpace() {
  const [status, setStatus] = useState<TraceStatus | null>(null);
  const [consent, setConsent] = useState<TraceConsent | null>(null);
  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const [usage, setUsage] = useState<AppUsage[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [s, cs] = await Promise.all([
        invoke<TraceStatus>("trace_status"),
        invoke<TraceConsent>("trace_consent_state"),
      ]);
      // Prevent unnecessary React re-renders if Tauri returns identical data.
      setStatus((prev) => (isEqual(prev, s) ? prev : s));
      setConsent((prev) => (isEqual(prev, cs) ? prev : cs));
      setError(null);
      if (s.total_events > 0) {
        // Heute 00:00 als "since" für die App-Nutzung.
        const since = Math.floor(new Date().setHours(0, 0, 0, 0) / 1000);
        const [ev, us] = await Promise.all([
          invoke<ActivityEvent[]>("trace_recent_events", { limit: 40 }),
          invoke<AppUsage[]>("trace_app_usage", { since }),
        ]);
        setEvents((prev) => (isEqual(prev, ev) ? prev : ev));
        setUsage((prev) => (isEqual(prev, us) ? prev : us));
      } else {
        const emptyEvents: ActivityEvent[] = [];
        const emptyUsage: AppUsage[] = [];
        setEvents((prev) => (isEqual(prev, emptyEvents) ? prev : emptyEvents));
        setUsage((prev) => (isEqual(prev, emptyUsage) ? prev : emptyUsage));
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, []);

  useEffect(() => {
    refresh();
    const id = window.setInterval(refresh, POLL_MS);
    return () => window.clearInterval(id);
  }, [refresh]);

  async function setConsentChoice(grant: boolean) {
    setBusy(true);
    setError(null);
    try {
      await invoke(grant ? "trace_consent_grant" : "trace_consent_revoke");
      await refresh();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }

  const capturing = status?.capturing ?? false;
  const granted = consent?.consent === "granted";

  return (
    <div style={styles.space}>
      <h1 style={styles.title}>Trace — Task-Mining</h1>
      <p style={styles.lead}>
        Erfasst, welche Apps und Fenster im Vordergrund sind, um automatisierbare
        Abläufe sichtbar zu machen. Inhalte (Tastatureingaben, Zwischenablage) werden
        lokal maskiert — übertragen werden nur die maskierten Aktivitäts-Signale.
      </p>

      {!granted ? (
        // CONSENT-GATE: ohne erteilte Einwilligung wird NICHTS erfasst. Transparenz zuerst.
        <section style={styles.consentCard}>
          <div style={styles.consentHead}>Einwilligung zur Aktivitäts-Erfassung</div>
          <p style={styles.consentLead}>
            Trace erfasst — nur mit deiner Einwilligung — maskierte Signale deiner PC-Nutzung, um
            automatisierbare Abläufe zu erkennen. Du kannst die Einwilligung jederzeit widerrufen.
          </p>
          <div style={styles.consentGrid}>
            <div style={styles.consentCol}>
              <div style={styles.consentColHeadOk}>✓ Was erfasst wird</div>
              {CAPTURED.map((t) => (
                <div key={t} style={styles.consentItem}>{t}</div>
              ))}
            </div>
            <div style={styles.consentCol}>
              <div style={styles.consentColHeadNo}>✕ Was NICHT erfasst wird</div>
              {NOT_CAPTURED.map((t) => (
                <div key={t} style={styles.consentItem}>{t}</div>
              ))}
            </div>
          </div>
          <button
            disabled={busy}
            onClick={() => setConsentChoice(true)}
            style={{ ...styles.toggle, background: "#06b6d4", border: "1px solid #06b6d4", color: "#0a1424", opacity: busy ? 0.6 : 1, alignSelf: "flex-start" }}
            type="button"
          >
            Ich willige ein und starte die Erfassung
          </button>
          {consent?.consent === "revoked" ? (
            <div style={styles.metaRow}><span>Einwilligung widerrufen — Erfassung ist gestoppt.</span></div>
          ) : null}
        </section>
      ) : (
        <section style={styles.controlCard}>
          <div style={styles.controlRow}>
            <span
              style={{
                ...styles.badge,
                background: capturing ? "rgba(6,182,212,0.16)" : "rgba(234,179,8,0.14)",
                border: capturing ? "1px solid rgba(6,182,212,0.55)" : "1px solid rgba(234,179,8,0.4)",
                color: capturing ? "#a5f3fc" : "#fde68a",
              }}
            >
              <span style={{ ...styles.dot, background: capturing ? "#06b6d4" : "#eab308" }} />
              {capturing ? "Erfassung läuft" : "Eingewilligt — startet…"}
            </span>
            <button
              disabled={busy}
              onClick={() => setConsentChoice(false)}
              style={{ ...styles.toggle, background: "#991b1b", border: "1px solid #991b1b", color: "#fff", opacity: busy ? 0.6 : 1 }}
              type="button"
            >
              Erfassung stoppen (Einwilligung widerrufen)
            </button>
          </div>
          <div style={styles.metaRow}>
            {consent && consent.consent_at > 0 ? <span>eingewilligt {fmtClock(consent.consent_at)}</span> : null}
            <span>{status?.total_events ?? 0} Episoden</span>
            {status?.last_event_at ? <span>zuletzt {fmtClock(status.last_event_at)}</span> : null}
          </div>
        </section>
      )}

      {error ? <div style={styles.error}>{error}</div> : null}

      {usage.length > 0 ? (
        <section style={styles.panel}>
          <div style={styles.panelHead}>Heute — Zeit pro App</div>
          {usage.slice(0, 8).map((u) => (
            <div key={u.app_name} style={styles.usageRow}>
              <span style={styles.usageApp}>{u.app_name || "—"}</span>
              <span style={styles.usageSecs}>{fmtDuration(u.total_secs)}</span>
            </div>
          ))}
        </section>
      ) : null}

      {events.length > 0 ? (
        <section style={styles.panel}>
          <div style={styles.panelHead}>Letzte Aktivität</div>
          {events.map((e) => (
            <div key={e.id} style={styles.eventRow}>
              <span style={styles.eventApp}>{e.app_name || "—"}</span>
              <span style={styles.eventTitle} title={e.window_title}>
                {e.window_title || "—"}
              </span>
              <span style={styles.eventDur}>{fmtDuration(e.duration_secs)}</span>
            </div>
          ))}
        </section>
      ) : null}

      {granted && !capturing && (status?.total_events ?? 0) === 0 ? (
        <div style={styles.empty}>
          <SonarLogo size={40} />
          <p style={styles.emptyText}>
            Noch keine Daten — die Erfassung läuft an und sammelt im Hintergrund.
          </p>
        </div>
      ) : null}
    </div>
  );
});

const styles: Record<string, CSSProperties> = {
  space: { display: "flex", flexDirection: "column", gap: 16, maxWidth: 680 },
  title: { fontSize: 22, fontWeight: 800, margin: 0 },
  lead: { color: "#94a3b8", fontSize: 14, lineHeight: "21px", margin: 0 },
  controlCard: {
    background: "rgba(15, 26, 44, 0.6)",
    border: "1px solid rgba(6, 182, 212, 0.18)",
    borderRadius: 12,
    boxSizing: "border-box",
    display: "flex",
    flexDirection: "column",
    gap: 12,
    padding: "18px 18px 16px",
  },
  controlRow: {
    alignItems: "center",
    display: "flex",
    gap: 14,
    justifyContent: "space-between",
  },
  badge: {
    alignItems: "center",
    borderRadius: 999,
    display: "inline-flex",
    fontSize: 13,
    fontWeight: 700,
    gap: 8,
    lineHeight: "18px",
    padding: "7px 11px",
  },
  dot: { borderRadius: "50%", height: 9, width: 9 },
  toggle: {
    borderRadius: 8,
    cursor: "pointer",
    fontSize: 13,
    fontWeight: 800,
    minHeight: 36,
    padding: "8px 16px",
  },
  metaRow: { color: "#64748b", display: "flex", fontSize: 12, gap: 16 },
  panel: {
    background: "rgba(15, 26, 44, 0.5)",
    border: "1px solid rgba(6, 182, 212, 0.14)",
    borderRadius: 12,
    boxSizing: "border-box",
    overflow: "hidden",
  },
  panelHead: {
    background: "rgba(6, 182, 212, 0.08)",
    color: "#64748b",
    fontSize: 11,
    fontWeight: 800,
    letterSpacing: 1,
    padding: "10px 14px",
    textTransform: "uppercase",
  },
  usageRow: {
    borderTop: "1px solid rgba(148,163,184,0.1)",
    display: "flex",
    fontSize: 14,
    justifyContent: "space-between",
    padding: "9px 14px",
  },
  usageApp: { color: "#ecfeff", fontWeight: 600 },
  usageSecs: { color: "#a5f3fc", fontWeight: 700 },
  eventRow: {
    alignItems: "baseline",
    borderTop: "1px solid rgba(148,163,184,0.1)",
    display: "flex",
    gap: 12,
    padding: "8px 14px",
  },
  eventApp: { color: "#ecfeff", flexShrink: 0, fontSize: 13, fontWeight: 700, width: 110 },
  eventTitle: {
    color: "#94a3b8",
    flex: 1,
    fontSize: 13,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  eventDur: { color: "#64748b", flexShrink: 0, fontSize: 12, fontWeight: 700 },
  error: {
    background: "rgba(220, 38, 38, 0.1)",
    border: "1px solid rgba(220, 38, 38, 0.3)",
    borderRadius: 8,
    color: "#fca5a5",
    fontSize: 13,
    padding: 12,
  },
  empty: {
    alignItems: "center",
    background: "rgba(15, 26, 44, 0.5)",
    border: "1px dashed rgba(6, 182, 212, 0.3)",
    borderRadius: 14,
    display: "flex",
    flexDirection: "column",
    gap: 12,
    padding: "36px 32px",
    textAlign: "center",
  },
  emptyText: { color: "#94a3b8", fontSize: 14, margin: 0, maxWidth: 360 },
  consentCard: {
    background: "rgba(15, 26, 44, 0.6)",
    border: "1px solid rgba(6, 182, 212, 0.28)",
    borderRadius: 12,
    boxSizing: "border-box",
    display: "flex",
    flexDirection: "column",
    gap: 14,
    padding: "18px 18px 16px",
  },
  consentHead: { color: "#ecfeff", fontSize: 15, fontWeight: 800 },
  consentLead: { color: "#94a3b8", fontSize: 13, lineHeight: "20px", margin: 0 },
  consentGrid: { display: "grid", gap: 14, gridTemplateColumns: "1fr 1fr" },
  consentCol: { display: "flex", flexDirection: "column", gap: 6 },
  consentColHeadOk: { color: "#a5f3fc", fontSize: 12, fontWeight: 800, letterSpacing: 0.4, textTransform: "uppercase" },
  consentColHeadNo: { color: "#fca5a5", fontSize: 12, fontWeight: 800, letterSpacing: 0.4, textTransform: "uppercase" },
  consentItem: { color: "#cbd5e1", fontSize: 12.5, lineHeight: "17px" },
};
