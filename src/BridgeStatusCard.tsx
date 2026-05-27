import type { CSSProperties } from "react";

export type BridgeStatus = {
  online: boolean;
  version: string | null;
  paired?: boolean | null;
};

export type ConsentState = {
  remote_access: string;
  session_grant?: unknown;
  pending_count?: number;
  last_session_active_at?: string | number | null;
  [key: string]: unknown;
};

type BridgeStatusCardProps = {
  status: BridgeStatus | null;
  consentState: ConsentState | null;
  lastChecked: Date | null;
  error: string | null;
  consentError: string | null;
  consentAction: string | null;
  helpActionPending: boolean;
  helpMessage: string | null;
  helpError: string | null;
  onRevoke: () => void;
  onResume: () => void;
  onHelpRequest: () => void;
};

const styles: Record<string, CSSProperties> = {
  card: {
    width: "min(440px, calc(100vw - 40px))",
    border: "1px solid rgba(6, 182, 212, 0.28)",
    borderRadius: 8,
    background: "rgba(255, 255, 255, 0.94)",
    boxShadow: "0 24px 70px rgba(0, 0, 0, 0.28)",
    color: "#0a1424",
    padding: 28,
  },
  header: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: 16,
    marginBottom: 28,
  },
  title: {
    margin: 0,
    fontSize: 28,
    lineHeight: "34px",
    fontWeight: 700,
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
  dot: {
    borderRadius: "50%",
    height: 9,
    width: 9,
  },
  sectionLabel: {
    color: "#476174",
    fontSize: 12,
    fontWeight: 700,
    letterSpacing: 0,
    lineHeight: "16px",
    marginBottom: 10,
    textTransform: "uppercase",
  },
  bridgeLine: {
    alignItems: "baseline",
    display: "flex",
    gap: 10,
    justifyContent: "space-between",
    marginBottom: 18,
  },
  bridgeName: {
    fontSize: 18,
    fontWeight: 700,
    lineHeight: "24px",
  },
  bridgeState: {
    fontSize: 15,
    fontWeight: 700,
    lineHeight: "22px",
  },
  detailGrid: {
    borderTop: "1px solid rgba(10, 20, 36, 0.11)",
    display: "grid",
    gap: 12,
    paddingTop: 18,
  },
  detailRow: {
    display: "flex",
    gap: 16,
    justifyContent: "space-between",
  },
  detailLabel: {
    color: "#476174",
    fontSize: 14,
    lineHeight: "20px",
  },
  detailValue: {
    color: "#0a1424",
    fontSize: 14,
    fontWeight: 700,
    lineHeight: "20px",
    textAlign: "right",
  },
  remotePanel: {
    borderTop: "1px solid rgba(10, 20, 36, 0.11)",
    display: "grid",
    gap: 14,
    marginTop: 20,
    paddingTop: 18,
  },
  remoteHeader: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: 10,
    justifyContent: "space-between",
  },
  remoteControls: {
    display: "flex",
    flexWrap: "wrap",
    gap: 8,
  },
  stopButton: {
    background: "#991b1b",
    border: "1px solid #991b1b",
    borderRadius: 8,
    boxShadow: "none",
    color: "#ffffff",
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
    boxShadow: "none",
    color: "#0a1424",
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
    boxShadow: "none",
    color: "#0a1424",
    fontSize: 13,
    fontWeight: 800,
    lineHeight: "18px",
    minHeight: 36,
    padding: "8px 11px",
  },
  error: {
    background: "rgba(220, 38, 38, 0.08)",
    border: "1px solid rgba(220, 38, 38, 0.2)",
    borderRadius: 8,
    color: "#991b1b",
    fontSize: 13,
    lineHeight: "18px",
    marginTop: 18,
    padding: 12,
  },
  info: {
    background: "rgba(6, 182, 212, 0.09)",
    border: "1px solid rgba(6, 182, 212, 0.24)",
    borderRadius: 8,
    color: "#075d6d",
    fontSize: 13,
    lineHeight: "18px",
    marginTop: 18,
    padding: 12,
  },
};

export function BridgeStatusCard({
  status,
  consentState,
  lastChecked,
  error,
  consentError,
  consentAction,
  helpActionPending,
  helpMessage,
  helpError,
  onRevoke,
  onResume,
  onHelpRequest,
}: BridgeStatusCardProps) {
  const online = status?.online === true;
  const stateLabel = online ? "Online" : "Offline";
  const badgeStyle: CSSProperties = {
    ...styles.badge,
    background: online ? "rgba(6, 182, 212, 0.13)" : "rgba(220, 38, 38, 0.1)",
    color: online ? "#075d6d" : "#991b1b",
  };
  const dotStyle: CSSProperties = {
    ...styles.dot,
    background: online ? "#06b6d4" : "#dc2626",
  };
  const checkedLabel = lastChecked
    ? lastChecked.toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      })
    : "Pending";
  const pairingLabel =
    status?.paired === undefined || status.paired === null
      ? "Unknown"
      : status.paired
        ? "Paired"
        : "Not paired";
  const remoteAccess = consentState?.remote_access;
  const remoteActive = remoteAccess === "active";
  const remoteRevoked = remoteAccess === "revoked";
  const remoteKnown = remoteActive || remoteRevoked;
  const remoteLabel = remoteActive
    ? "AKTIV"
    : remoteRevoked
      ? "GESTOPPT"
      : "UNBEKANNT";
  const remoteBadgeStyle: CSSProperties = {
    ...styles.badge,
    background: remoteActive
      ? "rgba(6, 182, 212, 0.13)"
      : "rgba(220, 38, 38, 0.1)",
    color: remoteActive ? "#075d6d" : "#991b1b",
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
    <section style={styles.card} aria-label="Bridge status">
      <div style={styles.header}>
        <h1 style={styles.title}>Bridge &amp; Fernzugriff</h1>
        <span style={badgeStyle}>
          <span style={dotStyle} />
          {stateLabel}
        </span>
      </div>

      <div style={styles.sectionLabel}>Bridge</div>
      <div style={styles.bridgeLine}>
        <div style={styles.bridgeName}>Subunit bridge</div>
        <div style={styles.bridgeState}>{stateLabel}</div>
      </div>

      <div style={styles.detailGrid}>
        <div style={styles.detailRow}>
          <span style={styles.detailLabel}>Version</span>
          <span style={styles.detailValue}>{status?.version ?? "Unknown"}</span>
        </div>
        <div style={styles.detailRow}>
          <span style={styles.detailLabel}>Pairing</span>
          <span style={styles.detailValue}>{pairingLabel}</span>
        </div>
        <div style={styles.detailRow}>
          <span style={styles.detailLabel}>Last check</span>
          <span style={styles.detailValue}>{checkedLabel}</span>
        </div>
      </div>

      <div style={styles.remotePanel}>
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
            <span style={styles.detailLabel}>Pending</span>
            <span style={styles.detailValue}>
              {consentState?.pending_count ?? "Unknown"}
            </span>
          </div>
          <div style={styles.detailRow}>
            <span style={styles.detailLabel}>u1 arbeitet</span>
            <span style={styles.detailValue}>
              {u1Active ? "Aktiv" : remoteKnown ? "Inaktiv" : "Unknown"}
            </span>
          </div>
        </div>
      </div>

      {helpMessage ? <div style={styles.info}>{helpMessage}</div> : null}
      {helpError ? <div style={styles.error}>{helpError}</div> : null}
      {error ? <div style={styles.error}>{error}</div> : null}
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
