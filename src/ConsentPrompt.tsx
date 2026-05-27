import { useEffect, useMemo, useState, type CSSProperties } from "react";

export type ConsentRequest = {
  id: string;
  operator_id?: string | null;
  cmd?: string[];
  cwd?: string | null;
  scope?: unknown;
  expires_at?: unknown;
  [key: string]: unknown;
};

type ConsentPromptProps = {
  request: ConsentRequest | null;
  actionPending: boolean;
  error: string | null;
  onAllow: (id: string) => void;
  onAllowRemember: (id: string) => void;
  onDeny: (id: string) => void;
  onExpired: (id: string) => void;
};

const styles: Record<string, CSSProperties> = {
  backdrop: {
    alignItems: "center",
    background: "rgba(10, 20, 36, 0.72)",
    display: "flex",
    inset: 0,
    justifyContent: "center",
    padding: 20,
    position: "fixed",
    zIndex: 20,
  },
  dialog: {
    background: "#ffffff",
    border: "1px solid rgba(6, 182, 212, 0.38)",
    borderRadius: 8,
    boxShadow: "0 28px 90px rgba(0, 0, 0, 0.42)",
    color: "#0a1424",
    maxHeight: "calc(100vh - 40px)",
    overflow: "auto",
    padding: 26,
    width: "min(680px, calc(100vw - 40px))",
  },
  header: {
    alignItems: "flex-start",
    display: "flex",
    gap: 18,
    justifyContent: "space-between",
    marginBottom: 22,
  },
  title: {
    fontSize: 24,
    fontWeight: 800,
    lineHeight: "30px",
    margin: 0,
    textAlign: "left",
  },
  countdown: {
    alignItems: "center",
    background: "rgba(6, 182, 212, 0.12)",
    border: "1px solid rgba(6, 182, 212, 0.25)",
    borderRadius: 999,
    color: "#075d6d",
    display: "inline-flex",
    fontSize: 13,
    fontWeight: 800,
    lineHeight: "18px",
    padding: "7px 11px",
    whiteSpace: "nowrap",
  },
  details: {
    display: "grid",
    gap: 14,
    marginBottom: 22,
  },
  detail: {
    display: "grid",
    gap: 6,
  },
  label: {
    color: "#476174",
    fontSize: 12,
    fontWeight: 800,
    letterSpacing: 0,
    lineHeight: "16px",
    textTransform: "uppercase",
  },
  value: {
    color: "#0a1424",
    fontSize: 15,
    fontWeight: 700,
    lineHeight: "21px",
    overflowWrap: "anywhere",
  },
  command: {
    background: "#0a1424",
    border: "1px solid rgba(6, 182, 212, 0.48)",
    borderRadius: 8,
    color: "#ecfeff",
    fontFamily:
      'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
    fontSize: 14,
    lineHeight: "20px",
    margin: 0,
    overflowX: "auto",
    padding: 14,
    whiteSpace: "pre-wrap",
    wordBreak: "break-word",
  },
  actions: {
    display: "flex",
    flexWrap: "wrap",
    gap: 10,
    justifyContent: "flex-end",
  },
  primaryButton: {
    background: "#06b6d4",
    border: "1px solid #06b6d4",
    borderRadius: 8,
    boxShadow: "none",
    color: "#0a1424",
    fontSize: 14,
    fontWeight: 800,
    lineHeight: "20px",
    minHeight: 40,
    padding: "9px 13px",
  },
  secondaryButton: {
    background: "#ffffff",
    border: "1px solid rgba(10, 20, 36, 0.2)",
    borderRadius: 8,
    boxShadow: "none",
    color: "#0a1424",
    fontSize: 14,
    fontWeight: 800,
    lineHeight: "20px",
    minHeight: 40,
    padding: "9px 13px",
  },
  dangerButton: {
    background: "#991b1b",
    border: "1px solid #991b1b",
    borderRadius: 8,
    boxShadow: "none",
    color: "#ffffff",
    fontSize: 14,
    fontWeight: 800,
    lineHeight: "20px",
    minHeight: 40,
    padding: "9px 13px",
  },
  error: {
    background: "rgba(220, 38, 38, 0.08)",
    border: "1px solid rgba(220, 38, 38, 0.2)",
    borderRadius: 8,
    color: "#991b1b",
    fontSize: 13,
    lineHeight: "18px",
    marginBottom: 16,
    padding: 12,
  },
};

export function ConsentPrompt({
  request,
  actionPending,
  error,
  onAllow,
  onAllowRemember,
  onDeny,
  onExpired,
}: ConsentPromptProps) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!request) {
      return;
    }

    setNow(Date.now());
    const intervalId = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(intervalId);
  }, [request?.id]);

  const expiresAt = useMemo(() => parseExpiry(request?.expires_at), [request]);
  const remainingMs = expiresAt === null ? null : expiresAt - now;

  useEffect(() => {
    if (request && remainingMs !== null && remainingMs <= 0) {
      onExpired(request.id);
    }
  }, [onExpired, remainingMs, request]);

  if (!request) {
    return null;
  }

  const command = request.cmd?.length ? request.cmd.join(" ") : "Unknown";
  const countdownLabel =
    remainingMs === null ? "Ablauf unbekannt" : formatRemaining(remainingMs);

  return (
    <div style={styles.backdrop}>
      <section
        aria-labelledby="consent-title"
        aria-modal="true"
        role="dialog"
        style={styles.dialog}
      >
        <div style={styles.header}>
          <h2 id="consent-title" style={styles.title}>
            Freigabe erforderlich
          </h2>
          <span style={styles.countdown}>{countdownLabel}</span>
        </div>

        <div style={styles.details}>
          <div style={styles.detail}>
            <span style={styles.label}>Operator</span>
            <span style={styles.value}>
              {request.operator_id || "Unbekannt"}
            </span>
          </div>

          <div style={styles.detail}>
            <span style={styles.label}>Kommando</span>
            <pre style={styles.command}>{command}</pre>
          </div>

          <div style={styles.detail}>
            <span style={styles.label}>Arbeitsverzeichnis</span>
            <span style={styles.value}>{request.cwd || "Unbekannt"}</span>
          </div>

          <div style={styles.detail}>
            <span style={styles.label}>Scope</span>
            <span style={styles.value}>{formatValue(request.scope)}</span>
          </div>
        </div>

        {error ? <div style={styles.error}>{error}</div> : null}

        <div style={styles.actions}>
          <button
            disabled={actionPending}
            onClick={() => onDeny(request.id)}
            style={styles.dangerButton}
            type="button"
          >
            Ablehnen
          </button>
          <button
            disabled={actionPending}
            onClick={() => onAllowRemember(request.id)}
            style={styles.secondaryButton}
            type="button"
          >
            Erlauben (30 Min)
          </button>
          <button
            disabled={actionPending}
            onClick={() => onAllow(request.id)}
            style={styles.primaryButton}
            type="button"
          >
            Erlauben
          </button>
        </div>
      </section>
    </div>
  );
}

function parseExpiry(value: unknown): number | null {
  if (!value) {
    return null;
  }

  if (typeof value === "number") {
    return value < 1_000_000_000_000 ? value * 1000 : value;
  }

  if (typeof value === "string") {
    const numeric = Number(value);
    if (Number.isFinite(numeric)) {
      return numeric < 1_000_000_000_000 ? numeric * 1000 : numeric;
    }

    const timestamp = Date.parse(value);
    return Number.isFinite(timestamp) ? timestamp : null;
  }

  return null;
}

function formatRemaining(value: number): string {
  if (value <= 0) {
    return "Abgelaufen";
  }

  const totalSeconds = Math.ceil(value / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

function formatValue(value: unknown): string {
  if (typeof value === "string" && value.length > 0) {
    return value;
  }

  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }

  if (value && typeof value === "object") {
    return JSON.stringify(value);
  }

  return "Unbekannt";
}
