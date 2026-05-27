import type { CSSProperties } from "react";

export type BridgeStatus = {
  online: boolean;
  version: string | null;
  paired?: boolean | null;
};

type BridgeCardProps = {
  status: BridgeStatus | null;
  lastChecked?: Date | null;
  error?: string | null;
};

/** Fundament-Karte: die Bridge ist die Verbindung der Maschine zu subunit und
 *  trägt ALLE Spaces (Forge + Trace) — daher App-Ebene (Home), nicht im Forge-Space. */
export function BridgeCard({ status, lastChecked, error }: BridgeCardProps) {
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
    : "—";
  const pairingLabel =
    status?.paired === undefined || status?.paired === null
      ? "Unbekannt"
      : status.paired
        ? "Gekoppelt"
        : "Nicht gekoppelt";

  return (
    <section style={styles.card} aria-label="Bridge (Fundament)">
      <div style={styles.header}>
        <div>
          <h2 style={styles.title}>Subunit Bridge</h2>
          <p style={styles.subtitle}>Fundament — verbindet diese Maschine mit subunit</p>
        </div>
        <span style={badgeStyle}>
          <span style={dotStyle} />
          {stateLabel}
        </span>
      </div>

      <div style={styles.detailGrid}>
        <div style={styles.detailRow}>
          <span style={styles.detailLabel}>Version</span>
          <span style={styles.detailValue}>{status?.version ?? "Unbekannt"}</span>
        </div>
        <div style={styles.detailRow}>
          <span style={styles.detailLabel}>Kopplung</span>
          <span style={styles.detailValue}>{pairingLabel}</span>
        </div>
        <div style={styles.detailRow}>
          <span style={styles.detailLabel}>Letzte Prüfung</span>
          <span style={styles.detailValue}>{checkedLabel}</span>
        </div>
      </div>

      <p style={styles.footnote}>Trägt Forge (Remote-Support) und Trace (Task-Mining).</p>
      {error ? <div style={styles.error}>{error}</div> : null}
    </section>
  );
}

const styles: Record<string, CSSProperties> = {
  card: {
    background: "rgba(255, 255, 255, 0.94)",
    border: "1px solid rgba(6, 182, 212, 0.28)",
    borderRadius: 12,
    boxShadow: "0 24px 70px rgba(0, 0, 0, 0.28)",
    boxSizing: "border-box",
    color: "#0a1424",
    padding: 24,
    width: "100%",
  },
  header: {
    alignItems: "flex-start",
    display: "flex",
    gap: 16,
    justifyContent: "space-between",
    marginBottom: 20,
  },
  title: { fontSize: 22, fontWeight: 800, lineHeight: "28px", margin: 0 },
  subtitle: { color: "#476174", fontSize: 13, lineHeight: "18px", margin: "4px 0 0" },
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
  detailGrid: {
    borderTop: "1px solid rgba(10, 20, 36, 0.11)",
    display: "grid",
    gap: 12,
    paddingTop: 18,
  },
  detailRow: { display: "flex", gap: 16, justifyContent: "space-between" },
  detailLabel: { color: "#476174", fontSize: 14, lineHeight: "20px" },
  detailValue: {
    color: "#0a1424",
    fontSize: 14,
    fontWeight: 700,
    lineHeight: "20px",
    textAlign: "right",
  },
  footnote: { color: "#476174", fontSize: 12, lineHeight: "17px", margin: "16px 0 0" },
  error: {
    background: "rgba(220, 38, 38, 0.08)",
    border: "1px solid rgba(220, 38, 38, 0.2)",
    borderRadius: 8,
    color: "#991b1b",
    fontSize: 13,
    lineHeight: "18px",
    marginTop: 14,
    padding: 12,
  },
};
