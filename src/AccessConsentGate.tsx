import { useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SonarLogo } from "./SonarLogo";

export type AccessConsentState = {
  consent: string; // "unset" | "full" | "confirm"
  decided: boolean;
  decided_at: number;
};

/**
 * Erster Onboarding-Schritt (Task #12 „Vollzugriff ab Werk"): der einmalige, explizite
 * Consent-Schritt GANZ AM ANFANG — vor Login und allen weiteren Schritten. Der Text ist
 * der von TJ freigegebene Wortlaut. Die Entscheidung wird sofort lokal persistiert
 * (Consent-Record mit Zeitstempel) und an die Bridge gemeldet (Audit: Gerät + App-Version);
 * ist die Bridge noch nicht erreichbar, holt der Reassert-Loop das automatisch nach.
 * Widerruf jederzeit: Forge-Space → Einstellungen → „Vollzugriff"-Schalter.
 */
export function AccessConsentGate({
  onDecided,
}: {
  onDecided: (state: AccessConsentState) => void;
}) {
  const [pending, setPending] = useState<"full" | "confirm" | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function decide(mode: "full" | "confirm") {
    setPending(mode);
    setError(null);
    try {
      const state = await invoke<AccessConsentState>("access_consent_set", {
        mode,
        source: "onboarding",
      });
      onDecided(state);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setPending(null);
    }
  }

  return (
    <main style={styles.splash}>
      <div style={styles.card}>
        <SonarLogo size={56} />
        <h1 style={styles.title}>Vollzugriff für dein Subunit-Team aktivieren</h1>
        <p style={styles.body}>
          Sonar verbindet dieses Gerät mit deinem Subunit-Arbeitsbereich. Mit deiner
          Zustimmung darf dein Subunit-Operator auf diesem Gerät Befehle ausführen, Dateien
          lesen und ändern sowie Software installieren und aktualisieren — ohne erneute
          Nachfrage bei jeder einzelnen Aktion.
        </p>
        <p style={styles.body}>
          Jede Aktion wird protokolliert und ist für dich jederzeit einsehbar. Du kannst den
          Vollzugriff jederzeit in den Einstellungen widerrufen — laufende Aktionen werden
          dann beendet.
        </p>
        <div style={styles.actions}>
          <button
            type="button"
            disabled={pending !== null}
            onClick={() => decide("full")}
            style={{ ...styles.primary, opacity: pending !== null ? 0.6 : 1 }}
          >
            {pending === "full" ? "Aktiviere…" : "Vollzugriff aktivieren"}
          </button>
          <button
            type="button"
            disabled={pending !== null}
            onClick={() => decide("confirm")}
            style={{ ...styles.secondary, opacity: pending !== null ? 0.6 : 1 }}
          >
            {pending === "confirm" ? "Speichere…" : "Nur mit Bestätigung pro Aktion"}
          </button>
        </div>
        {error ? <p style={styles.error}>{error}</p> : null}
        <p style={styles.powered}>Powered by Subunit Bridge</p>
      </div>
    </main>
  );
}

const fontStack =
  'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif';

const styles: Record<string, CSSProperties> = {
  splash: {
    alignItems: "center",
    background: "linear-gradient(160deg, #0a1424 0%, #0e2333 62%, #062a36 100%)",
    color: "#ecfeff",
    display: "flex",
    fontFamily: fontStack,
    inset: 0,
    justifyContent: "center",
    position: "fixed",
  },
  card: {
    alignItems: "center",
    background: "linear-gradient(180deg, rgba(15, 26, 44, 0.92), rgba(8, 16, 30, 0.92))",
    border: "1px solid #06b6d4",
    borderRadius: 16,
    boxShadow: "0 32px 110px rgba(0,0,0,0.55), 0 0 52px rgba(6,182,212,0.12)",
    boxSizing: "border-box",
    display: "flex",
    flexDirection: "column",
    gap: 14,
    padding: "36px 40px 30px",
    textAlign: "center",
    width: "min(520px, calc(100vw - 48px))",
  },
  title: {
    fontSize: 22,
    fontWeight: 800,
    lineHeight: "30px",
    margin: "4px 0 0",
  },
  body: {
    color: "#94a3b8",
    fontSize: 14,
    lineHeight: "21px",
    margin: 0,
    maxWidth: 430,
    textAlign: "left",
  },
  actions: {
    display: "flex",
    flexDirection: "column",
    gap: 10,
    marginTop: 8,
    width: "100%",
  },
  primary: {
    background: "#06b6d4",
    border: "none",
    borderRadius: 10,
    color: "#0f172a",
    cursor: "pointer",
    fontSize: 15,
    fontWeight: 800,
    padding: "12px 22px",
    width: "100%",
  },
  secondary: {
    background: "transparent",
    border: "1px solid rgba(148, 163, 184, 0.4)",
    borderRadius: 10,
    color: "#cbd5e1",
    cursor: "pointer",
    fontSize: 14,
    fontWeight: 600,
    padding: "11px 22px",
    width: "100%",
  },
  error: { color: "#fca5a5", fontSize: 13, margin: 0 },
  powered: {
    color: "#475569",
    fontSize: 10,
    fontWeight: 800,
    letterSpacing: 1,
    margin: "10px 0 0",
    textTransform: "uppercase",
  },
};
