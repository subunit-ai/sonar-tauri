import type { CSSProperties } from "react";

export type ConsentState = {
  remote_access: string;
  session_grant?: unknown;
  pending_count?: number;
  last_session_active_at?: string | number | null;
  [key: string]: unknown;
};

type ForgeAccessCardProps = {
  consentState: ConsentState | null;
  consentError: string | null;
  consentAction: string | null;
  helpActionPending: boolean;
  helpMessage: string | null;
  helpError: string | null;
  bridgeOnline: boolean;
  onRevoke: () => void;
  onResume: () => void;
  onHelpRequest: () => void;
};

/** Forge = die ausführende Instanz auf der Bridge: Remote-Zugriff, Consent, Stop, Hilfe.
 *  Zeigt bewusst KEINEN Bridge-Verbindungsstatus (das ist Fundament → Home/Sidebar). */
export function ForgeAccessCard({
  consentState,
  consentError,
  consentAction,
  helpActionPending,
  helpMessage,
  helpError,
  bridgeOnline,
  onRevoke,
  onResume,
  onHelpRequest,
}: ForgeAccessCardProps) {
  const remoteAccess = consentState?.remote_access;
  const remoteActive = remoteAccess === "active";
  const remoteRevoked = remoteAccess === "revoked";
  const remoteKnown = remoteActive || remoteRevoked;
  const remoteLabel = remoteActive ? "AKTIV" : remoteRevoked ? "GESTOPPT" : "UNBEKANNT";
  const remoteBadgeStyle: CSSProperties = {
    ...styles.badge,
    background: remoteActive ? "rgba(6, 182, 212, 0.13)" : "rgba(220, 38, 38, 0.14)",
    border: remoteActive ? "1px solid rgba(6, 182, 212, 0.45)" : "1px solid rgba(220, 38, 38, 0.4)",
    color: remoteActive ? "#a5f3fc" : "#fca5a5",
  };
  const remoteDotStyle: CSSProperties = {
    ...styles.dot,
    background: remoteActive ? "#06b6d4" : "#dc2626",
  };
  const lastActiveAt = parseLastActiveAt(consentState?.last_session_active_at);
  const u1Active =
    lastActiveAt !== null &&
    Date.now() >= lastActiveAt &&
    Date.now() - lastActiveAt < 40_000;

  return (
    <section style={styles.card} aria-label="Forge — Remote-Support">
      <div style={styles.remoteHeader}>
        <span style={remoteBadgeStyle}>
          <span style={remoteDotStyle} />
          Remote-Zugriff: {remoteLabel}
        </span>

        <div style={styles.remoteControls}>
          <button
            disabled={helpActionPending}
            onClick={onHelpRequest}
            style={styles.helpButton}
            type="button"
          >
            Hilfe anfordern
          </button>
          <button
            disabled={consentAction === "consent_revoke"}
            onClick={onRevoke}
            style={styles.stopButton}
            type="button"
          >
            Stop (Zugriff sperren)
          </button>
          {remoteRevoked ? (
            <button
              disabled={consentAction === "consent_resume"}
              onClick={onResume}
              style={styles.resumeButton}
              type="button"
            >
              Fortsetzen
            </button>
          ) : null}
        </div>
      </div>

      <div style={styles.detailGrid}>
        <div style={styles.detailRow}>
          <span style={styles.detailLabel}>Offene Freigaben</span>
          <span style={styles.detailValue}>{consentState?.pending_count ?? "—"}</span>
        </div>
        <div style={styles.detailRow}>
          <span style={styles.detailLabel}>u1 arbeitet</span>
          <span style={styles.detailValue}>
            {u1Active ? "Aktiv" : remoteKnown ? "Inaktiv" : "—"}
          </span>
        </div>
      </div>

      {!bridgeOnline ? (
        <div style={styles.warn}>
          Bridge offline — Remote-Support ist erst verfügbar, wenn die Bridge verbunden ist (siehe Home).
        </div>
      ) : null}
      {helpMessage ? <div style={styles.info}>{helpMessage}</div> : null}
      {helpError ? <div style={styles.error}>{helpError}</div> : null}
      {consentError ? <div style={styles.error}>{consentError}</div> : null}
    </section>
  );
}

function parseLastActiveAt(value: string | number | null | undefined): number | null {
  if (typeof value === "number") {
    return value < 1_000_000_000_000 ? value * 1000 : value;
  }
  if (typeof value === "string") {
    const numeric = Number(value);
    if (Number.isFinite(numeric)) {
      return numeric < 1_000_000_000_000 ? numeric * 1000 : numeric;
    }
    const parsed = Date.parse(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

const styles: Record<string, CSSProperties> = {
  card: {
    background: "rgba(13, 22, 38, 0.72)",
    backdropFilter: "blur(18px) saturate(1.4)",
    WebkitBackdropFilter: "blur(18px) saturate(1.4)",
    border: "1px solid rgba(6, 182, 212, 0.18)",
    borderRadius: 12,
    boxShadow: "0 24px 70px rgba(0, 0, 0, 0.45), inset 0 1px 0 rgba(190, 215, 245, 0.06)",
    boxSizing: "border-box",
    color: "#e2e8f0",
    display: "grid",
    gap: 16,
    padding: 24,
    width: "100%",
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
    whiteSpace: "nowrap",
  },
  dot: { borderRadius: "50%", height: 9, width: 9 },
  remoteHeader: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: 10,
    justifyContent: "space-between",
  },
  remoteControls: { display: "flex", flexWrap: "wrap", gap: 8 },
  stopButton: {
    background: "rgba(220, 38, 38, 0.16)",
    border: "1px solid rgba(220, 38, 38, 0.5)",
    borderRadius: 8,
    color: "#fca5a5",
    cursor: "pointer",
    fontSize: 13,
    fontWeight: 800,
    lineHeight: "18px",
    minHeight: 36,
    padding: "8px 11px",
  },
  helpButton: {
    background: "#06b6d4",
    border: "1px solid #06b6d4",
    borderRadius: 8,
    color: "#03121f",
    cursor: "pointer",
    fontSize: 13,
    fontWeight: 800,
    lineHeight: "18px",
    minHeight: 36,
    padding: "8px 11px",
  },
  resumeButton: {
    background: "#06b6d4",
    border: "1px solid #06b6d4",
    borderRadius: 8,
    color: "#03121f",
    cursor: "pointer",
    fontSize: 13,
    fontWeight: 800,
    lineHeight: "18px",
    minHeight: 36,
    padding: "8px 11px",
  },
  detailGrid: {
    borderTop: "1px solid rgba(148, 163, 184, 0.12)",
    display: "grid",
    gap: 12,
    paddingTop: 16,
  },
  detailRow: { display: "flex", gap: 16, justifyContent: "space-between" },
  detailLabel: { color: "#94a3b8", fontSize: 14, lineHeight: "20px" },
  detailValue: {
    color: "#e2e8f0",
    fontSize: 14,
    fontWeight: 700,
    lineHeight: "20px",
    textAlign: "right",
  },
  warn: {
    background: "rgba(245, 158, 11, 0.13)",
    border: "1px solid rgba(245, 158, 11, 0.34)",
    borderRadius: 8,
    color: "#fde68a",
    fontSize: 13,
    lineHeight: "18px",
    padding: 12,
  },
  info: {
    background: "rgba(6, 182, 212, 0.1)",
    border: "1px solid rgba(6, 182, 212, 0.28)",
    borderRadius: 8,
    color: "#a5f3fc",
    fontSize: 13,
    lineHeight: "18px",
    padding: 12,
  },
  error: {
    background: "rgba(220, 38, 38, 0.12)",
    border: "1px solid rgba(220, 38, 38, 0.3)",
    borderRadius: 8,
    color: "#fca5a5",
    fontSize: 13,
    lineHeight: "18px",
    padding: 12,
  },
};
