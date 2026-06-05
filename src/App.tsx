import {
  useCallback,
  useEffect,
  useState,
  memo,
  type CSSProperties,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { BridgeCard, type BridgeStatus } from "./BridgeCard";
import { ForgeAccessCard, type ConsentState } from "./ForgeAccessCard";
import { ConsentPrompt, type ConsentRequest } from "./ConsentPrompt";
import { SonarLogo } from "./SonarLogo";
import { TraceSpace } from "./TraceSpace";

const BRIDGE_POLL_INTERVAL_MS = 3000;
const CONSENT_POLL_INTERVAL_MS = 1000;
const ACCOUNT_POLL_INTERVAL_MS = 5000;
const WINDOW_LABEL = currentWindowLabel();

const INK = "#ecfeff";
const CYAN = "#06b6d4";
const NAVY = "#0f172a";

type HelpRequestResult = {
  delivered: boolean;
  via: string;
  message: string;
};

type OverlayState = {
  operator: string;
};

type AccountState = {
  logged_in: boolean;
  email: string;
  is_operator: boolean;
  workspace_id: string;
};

type SpaceId = "home" | "forge" | "trace";

function App() {
  if (WINDOW_LABEL === "overlay") {
    return <OverlayWindow />;
  }

  if (WINDOW_LABEL === "overlayControl") {
    return <OverlayControl />;
  }

  return <MainApp />;
}

/** Login-Gate: erst Account prüfen, dann Shell. */
function MainApp() {
  const [account, setAccount] = useState<AccountState | null>(null);
  const [ready, setReady] = useState(false);

  const refreshAccount = useCallback(async () => {
    try {
      const next = await invoke<AccountState>("account_state");
      // Prevent unnecessary React re-renders if Tauri returns identical data.
      setAccount((prev) => (JSON.stringify(prev) === JSON.stringify(next) ? prev : next));
      return next;
    } catch {
      const fallback: AccountState = { logged_in: false, email: "", is_operator: false, workspace_id: "" };
      setAccount((prev) => (JSON.stringify(prev) === JSON.stringify(fallback) ? prev : fallback));
      return null;
    } finally {
      setReady(true);
    }
  }, []);

  useEffect(() => {
    refreshAccount();
    const id = window.setInterval(refreshAccount, ACCOUNT_POLL_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, [refreshAccount]);

  if (!ready) {
    return (
      <main style={shellStyles.splash}>
        <SonarLogo size={64} pulsing />
      </main>
    );
  }

  if (!account?.logged_in) {
    return <LoginScreen onLoggedIn={(next) => setAccount(next)} />;
  }

  return <Shell account={account} onAccountChange={refreshAccount} />;
}

function LoginScreen({
  onLoggedIn,
}: {
  onLoggedIn: (account: AccountState) => void;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function login() {
    setPending(true);
    setError(null);
    try {
      const account = await invoke<AccountState>("account_login");
      if (account.logged_in) {
        onLoggedIn(account);
      } else {
        setError("Login nicht abgeschlossen.");
      }
    } catch (caught) {
      setError(formatError(caught));
    } finally {
      setPending(false);
    }
  }

  return (
    <main style={shellStyles.splash}>
      <div style={loginStyles.card}>
        <SonarLogo size={72} pulsing />
        <h1 style={loginStyles.title}>Sonar</h1>
        <p style={loginStyles.subtitle}>
          Der Agent auf der Maschine — Forge &amp; Trace in einer App.
        </p>
        <button
          disabled={pending}
          onClick={login}
          style={{ ...loginStyles.button, opacity: pending ? 0.6 : 1 }}
          type="button"
        >
          {pending ? "Browser geöffnet — warte auf Login…" : "Mit Subunit anmelden"}
        </button>
        {error ? <p style={loginStyles.error}>{error}</p> : null}
        <p style={loginStyles.hint}>
          Login läuft über auth.subunit.ai in deinem Browser.
        </p>
        <p style={loginStyles.powered}>Powered by Subunit Bridge</p>
      </div>
    </main>
  );
}

const SPACES: { id: SpaceId; label: string; hint: string }[] = [
  { id: "home", label: "Home", hint: "Überblick" },
  { id: "forge", label: "Forge", hint: "Remote-Support" },
  { id: "trace", label: "Trace", hint: "Task-Mining" },
];

function Shell({
  account,
  onAccountChange,
}: {
  account: AccountState;
  onAccountChange: () => void;
}) {
  const [space, setSpace] = useState<SpaceId>("home");
  const [status, setStatus] = useState<BridgeStatus | null>(null);
  const [lastChecked, setLastChecked] = useState<Date | null>(null);

  // Bridge-Status app-weit pollen — die Bridge ist das FUNDAMENT der ganzen App
  // (Sidebar-Indikator + Home-Karte), nicht Teil eines einzelnen Spaces.
  useEffect(() => {
    let cancelled = false;
    let timeoutId: number | undefined;
    async function refreshStatus() {
      try {
        const next = await invoke<BridgeStatus>("bridge_status");
        if (!cancelled) {
          // Prevent unnecessary React re-renders if Tauri returns identical data.
          setStatus((prev) => (JSON.stringify(prev) === JSON.stringify(next) ? prev : next));
          setLastChecked(new Date());
        }
      } catch {
        if (!cancelled) {
          const fallback: BridgeStatus = { online: false, version: null, paired: null };
          setStatus((prev) => (JSON.stringify(prev) === JSON.stringify(fallback) ? prev : fallback));
          setLastChecked(new Date());
        }
      } finally {
        if (!cancelled) {
          timeoutId = window.setTimeout(refreshStatus, BRIDGE_POLL_INTERVAL_MS);
        }
      }
    }
    refreshStatus();
    return () => {
      cancelled = true;
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
    };
  }, []);

  async function logout() {
    // Fail-closed: nur bei erfolgreichem account_logout (Bridge entkoppelt) die UI als abgemeldet
    // aktualisieren. Schlägt es fehl, bleibt der Account sichtbar eingeloggt + Hinweis — sonst
    // glaubt der Nutzer, abgemeldet zu sein, während die Bridge gepairt bleibt. (Codex-ReReview P0)
    try {
      await invoke("account_logout");
      onAccountChange();
    } catch (caught) {
      console.error(caught);
      window.alert(
        "Abmelden fehlgeschlagen — die lokale Bridge konnte nicht entkoppelt werden. Bitte erneut versuchen.",
      );
    }
  }

  const bridgeOnline = status?.online ?? false;

  return (
    <div style={shellStyles.root}>
      <nav style={shellStyles.sidebar}>
        <div style={shellStyles.brand}>
          <SonarLogo size={30} />
          <span style={shellStyles.brandWord}>SONAR</span>
        </div>

        <div style={shellStyles.bridgeIndicator} title="Die Bridge ist das Fundament der App">
          <span
            style={{
              ...shellStyles.bridgeDot,
              background: bridgeOnline ? CYAN : "#f59e0b",
            }}
          />
          {bridgeOnline ? "Bridge verbunden" : "Bridge verbindet…"}
        </div>

        <div style={shellStyles.navList}>
          {SPACES.map((item) => {
            const active = item.id === space;
            return (
              <button
                key={item.id}
                onClick={() => setSpace(item.id)}
                style={{
                  ...shellStyles.navItem,
                  ...(active ? shellStyles.navItemActive : null),
                }}
                type="button"
              >
                <span style={shellStyles.navLabel}>{item.label}</span>
                <span style={shellStyles.navHint}>{item.hint}</span>
              </button>
            );
          })}
        </div>

        <div style={shellStyles.account}>
          <div style={shellStyles.accountEmail} title={account.email}>
            {account.email ? displayName(account.email) : "Angemeldet"}
          </div>
          <div
            style={{
              ...shellStyles.roleChip,
              ...(account.is_operator ? shellStyles.roleOperator : shellStyles.roleCustomer),
            }}
          >
            {account.is_operator ? "subunit · voller Zugriff" : "Kunde"}
          </div>
          <button onClick={logout} style={shellStyles.logout} type="button">
            Abmelden
          </button>
        </div>
      </nav>

      <section style={shellStyles.content}>
        {space === "home" ? (
          <HomeSpace account={account} status={status} lastChecked={lastChecked} />
        ) : null}
        {space === "forge" ? <ForgeSpace bridgeOnline={bridgeOnline} /> : null}
        {space === "trace" ? <TraceSpace /> : null}
      </section>
    </div>
  );
}

// Anzeigename aus der E-Mail ableiten: "finn.jedlitschka@subunit.ai" → "Finn Jedlitschka".
// Wir zeigen den Namen, nicht die ganze Adresse (die bleibt höchstens im Tooltip).
function displayName(email: string): string {
  const local = (email.split("@")[0] ?? "").trim();
  if (!local) return "";
  return local
    .split(/[._-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function SpaceShell({ children }: { children: ReactNode }) {
  return <div style={shellStyles.space}>{children}</div>;
}

const HomeSpace = memo(function HomeSpace({
  account,
  status,
  lastChecked,
}: {
  account: AccountState;
  status: BridgeStatus | null;
  lastChecked: Date | null;
}) {
  return (
    <SpaceShell>
      <h1 style={shellStyles.spaceTitle}>
        Willkommen{account.email ? `, ${displayName(account.email)}` : ""}
      </h1>
      <p style={shellStyles.spaceLead}>
        Sonar bündelt deine On-Site-Werkzeuge in einer App. Die Bridge ist das Fundament —
        Forge und Trace laufen darauf.
      </p>

      <BridgeCard status={status} lastChecked={lastChecked} />

      <div style={homeStyles.cards}>
        <div style={homeStyles.card}>
          <div style={homeStyles.cardHead}>Forge</div>
          <div style={homeStyles.cardValue}>Remote-Support</div>
          <div style={homeStyles.cardSub}>
            {account.is_operator ? "voller Zugriff (intern)" : "Freigabe nötig (Kunde)"}
          </div>
        </div>
        <div style={homeStyles.card}>
          <div style={homeStyles.cardHead}>Trace</div>
          <div style={homeStyles.cardValue}>Task-Mining</div>
          <div style={homeStyles.cardSub}>bald verfügbar</div>
        </div>
      </div>
    </SpaceShell>
  );
});

/** Forge-Space = die ausführende Instanz AUF der Bridge: Remote-Zugriff, Consent, Stop, Hilfe.
 *  Zeigt bewusst KEINEN Bridge-Verbindungsstatus (das ist Fundament → Home/Sidebar). */
const ForgeSpace = memo(function ForgeSpace({ bridgeOnline }: { bridgeOnline: boolean }) {
  const [consentState, setConsentState] = useState<ConsentState | null>(null);
  const [pendingConsentRequests, setPendingConsentRequests] = useState<
    ConsentRequest[]
  >([]);
  const [consentError, setConsentError] = useState<string | null>(null);
  const [consentActionError, setConsentActionError] = useState<string | null>(
    null,
  );
  const [consentAction, setConsentAction] = useState<string | null>(null);
  const [helpActionPending, setHelpActionPending] = useState(false);
  const [helpMessage, setHelpMessage] = useState<string | null>(null);
  const [helpError, setHelpError] = useState<string | null>(null);

  const refreshConsentState = useCallback(async () => {
    const nextState = await invoke<ConsentState>("consent_state");
    // Prevent unnecessary React re-renders if Tauri returns identical data.
    setConsentState((prev) => (JSON.stringify(prev) === JSON.stringify(nextState) ? prev : nextState));
    setConsentError(null);
    if ((nextState.pending_count ?? 0) === 0) {
      setPendingConsentRequests([]);
    }
    return nextState;
  }, []);

  const upsertPendingConsentRequest = useCallback((request: ConsentRequest) => {
    if (!request.id) {
      return;
    }
    setPendingConsentRequests((current) => {
      const existingIndex = current.findIndex((row) => row.id === request.id);
      if (existingIndex === -1) {
        return [...current, request];
      }
      const next = [...current];
      next[existingIndex] = request;
      return next;
    });
  }, []);

  const removePendingConsentRequest = useCallback((id: string) => {
    setPendingConsentRequests((current) =>
      current.filter((request) => request.id !== id),
    );
  }, []);

  useEffect(() => {
    let cancelled = false;
    let timeoutId: number | undefined;
    async function refresh() {
      try {
        const nextState = await invoke<ConsentState>("consent_state");
        if (!cancelled) {
          setConsentState((prev) => (JSON.stringify(prev) === JSON.stringify(nextState) ? prev : nextState));
          setConsentError(null);
          if ((nextState.pending_count ?? 0) === 0) {
            setPendingConsentRequests([]);
          }
        }
      } catch (caught) {
        if (!cancelled) setConsentError(formatError(caught));
      } finally {
        if (!cancelled) {
          timeoutId = window.setTimeout(refresh, CONSENT_POLL_INTERVAL_MS);
        }
      }
    }
    refresh();
    return () => {
      cancelled = true;
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlistenRequest: (() => void) | undefined;
    let unlistenState: (() => void) | undefined;

    listen<ConsentRequest>("consent://request", (event) => {
      upsertPendingConsentRequest(event.payload);
      setConsentActionError(null);
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else unlistenRequest = unlisten;
    });

    listen<ConsentState>("consent://state", (event) => {
      setConsentState((prev) => (JSON.stringify(prev) === JSON.stringify(event.payload) ? prev : event.payload));
      setConsentError(null);
      if ((event.payload.pending_count ?? 0) === 0) {
        setPendingConsentRequests([]);
      }
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else unlistenState = unlisten;
    });

    return () => {
      cancelled = true;
      unlistenRequest?.();
      unlistenState?.();
    };
  }, [upsertPendingConsentRequest]);

  async function decideConsent(
    id: string,
    command: "consent_allow" | "consent_deny",
    rememberForSeconds?: number,
  ) {
    setConsentAction(command);
    setConsentActionError(null);
    try {
      if (command === "consent_allow") {
        await invoke(command, { id, rememberForSeconds: rememberForSeconds ?? null });
      } else {
        await invoke(command, { id });
      }
      removePendingConsentRequest(id);
      await refreshConsentState();
    } catch (caught) {
      setConsentActionError(formatError(caught));
    } finally {
      setConsentAction(null);
    }
  }

  async function revokeConsent() {
    setConsentAction("consent_revoke");
    setConsentError(null);
    try {
      await invoke("consent_revoke");
      setPendingConsentRequests([]);
      await refreshConsentState();
    } catch (caught) {
      setConsentError(formatError(caught));
    } finally {
      setConsentAction(null);
    }
  }

  async function resumeConsent() {
    setConsentAction("consent_resume");
    setConsentError(null);
    try {
      await invoke("consent_resume");
      await refreshConsentState();
    } catch (caught) {
      setConsentError(formatError(caught));
    } finally {
      setConsentAction(null);
    }
  }

  async function requestHelp() {
    setHelpActionPending(true);
    setHelpMessage(null);
    setHelpError(null);
    try {
      const result = await invoke<HelpRequestResult>("help_request");
      setHelpMessage(result.message);
    } catch (caught) {
      setHelpError(formatError(caught));
    } finally {
      setHelpActionPending(false);
    }
  }

  const activeConsentRequest = pendingConsentRequests[0] ?? null;
  const promptActionPending =
    consentAction === "consent_allow" || consentAction === "consent_deny";

  return (
    <SpaceShell>
      <h1 style={shellStyles.spaceTitle}>Forge — Remote-Support</h1>
      <p style={shellStyles.spaceLead}>
        Die ausführende Instanz auf der Bridge. Hier steuerst du Fernzugriff, Freigaben und Stop.
      </p>
      <ForgeAccessCard
        bridgeOnline={bridgeOnline}
        consentState={consentState}
        consentError={consentError}
        consentAction={consentAction}
        helpActionPending={helpActionPending}
        helpMessage={helpMessage}
        helpError={helpError}
        onResume={resumeConsent}
        onRevoke={revokeConsent}
        onHelpRequest={requestHelp}
      />
      <ConsentPrompt
        actionPending={promptActionPending}
        error={consentActionError}
        onAllow={(id) => decideConsent(id, "consent_allow")}
        onAllowRemember={(id) => decideConsent(id, "consent_allow", 1800)}
        onDeny={(id) => decideConsent(id, "consent_deny")}
        onExpired={removePendingConsentRequest}
        request={activeConsentRequest}
      />
    </SpaceShell>
  );
});

function OverlayWindow() {
  const [operator, setOperator] = useState("u1");

  useEffect(() => {
    getCurrentWindow().setIgnoreCursorEvents(true).catch(console.error);
    let cancelled = false;
    let unlistenState: (() => void) | undefined;
    listen<OverlayState>("overlay://state", (event) => {
      const nextOperator = event.payload.operator.trim();
      if (nextOperator) setOperator(nextOperator);
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else unlistenState = unlisten;
    });
    return () => {
      cancelled = true;
      unlistenState?.();
    };
  }, []);

  return (
    <main style={overlayStyles.root} aria-label="u1 arbeitet">
      <style>{overlayCss}</style>
      <section style={overlayStyles.card}>
        <div style={overlayStyles.livePill}>
          <span className="sonar-live-dot" />
          LIVE
        </div>

        <div className="sonar-ping-scene" aria-hidden="true">
          <span className="sonar-ping ring1" />
          <span className="sonar-ping ring2" />
          <span className="sonar-ping ring3" />
          <span className="sonar-ping-core" />
        </div>

        <h1 style={overlayStyles.title}>{operator} arbeitet an deinem Gerät</h1>
        <p style={overlayStyles.subtitle}>
          Bitte einen Moment nicht eingreifen - du siehst alles live mit. Sobald
          fertig, verschwindet diese Anzeige von selbst.
        </p>
        <div style={overlayStyles.footer}>SUBUNIT · Sonar</div>
      </section>
    </main>
  );
}

function OverlayControl() {
  const [pending, setPending] = useState(false);

  async function dismiss() {
    setPending(true);
    try {
      await invoke("overlay_dismiss");
    } catch (caught) {
      console.error(caught);
      setPending(false);
    }
  }

  return (
    <main style={overlayControlStyles.root}>
      <button
        disabled={pending}
        onClick={dismiss}
        style={overlayControlStyles.button}
        type="button"
      >
        ✕ Ausblenden
      </button>
    </main>
  );
}

function currentWindowLabel(): string {
  try {
    return getCurrentWindow().label;
  } catch {
    return "main";
  }
}

function formatError(caught: unknown): string {
  return caught instanceof Error ? caught.message : String(caught);
}

export default App;

const fontStack =
  'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif';

const shellStyles: Record<string, CSSProperties> = {
  splash: {
    alignItems: "center",
    background: "linear-gradient(160deg, #0a1424 0%, #0e2333 62%, #062a36 100%)",
    color: INK,
    display: "flex",
    fontFamily: fontStack,
    inset: 0,
    justifyContent: "center",
    position: "fixed",
  },
  root: {
    background: "linear-gradient(160deg, #0a1424 0%, #0e2333 62%, #062a36 100%)",
    color: INK,
    display: "flex",
    fontFamily: fontStack,
    inset: 0,
    position: "fixed",
  },
  sidebar: {
    background: "rgba(3, 11, 24, 0.55)",
    borderRight: "1px solid rgba(6, 182, 212, 0.16)",
    boxSizing: "border-box",
    display: "flex",
    flexDirection: "column",
    gap: 14,
    minWidth: 212,
    padding: "20px 16px",
    width: 212,
  },
  brand: { alignItems: "center", display: "flex", gap: 10, padding: "4px 6px 0" },
  brandWord: { fontSize: 17, fontWeight: 800, letterSpacing: 3 },
  bridgeIndicator: {
    alignItems: "center",
    color: "#94a3b8",
    display: "flex",
    fontSize: 11,
    fontWeight: 600,
    gap: 7,
    padding: "0 6px 4px",
  },
  bridgeDot: { borderRadius: "50%", height: 8, width: 8 },
  navList: { display: "flex", flexDirection: "column", flex: 1, gap: 6 },
  navItem: {
    background: "transparent",
    border: "1px solid transparent",
    borderRadius: 10,
    color: "#cbd5e1",
    cursor: "pointer",
    display: "flex",
    flexDirection: "column",
    gap: 2,
    padding: "10px 12px",
    textAlign: "left",
  },
  navItemActive: {
    background: "rgba(6, 182, 212, 0.12)",
    border: "1px solid rgba(6, 182, 212, 0.5)",
    color: INK,
  },
  navLabel: { fontSize: 14, fontWeight: 700 },
  navHint: { color: "#64748b", fontSize: 11 },
  account: {
    borderTop: "1px solid rgba(148, 163, 184, 0.16)",
    display: "flex",
    flexDirection: "column",
    gap: 8,
    paddingTop: 14,
  },
  accountEmail: {
    fontSize: 12,
    fontWeight: 600,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  roleChip: {
    alignSelf: "flex-start",
    borderRadius: 999,
    fontSize: 10,
    fontWeight: 800,
    letterSpacing: 0.4,
    padding: "3px 9px",
    textTransform: "uppercase",
  },
  roleOperator: {
    background: "rgba(6, 182, 212, 0.16)",
    border: "1px solid rgba(6, 182, 212, 0.55)",
    color: "#a5f3fc",
  },
  roleCustomer: {
    background: "rgba(148, 163, 184, 0.14)",
    border: "1px solid rgba(148, 163, 184, 0.4)",
    color: "#cbd5e1",
  },
  logout: {
    background: "transparent",
    border: "1px solid rgba(148, 163, 184, 0.3)",
    borderRadius: 8,
    color: "#94a3b8",
    cursor: "pointer",
    fontSize: 12,
    fontWeight: 600,
    padding: "7px 10px",
  },
  content: { boxSizing: "border-box", flex: 1, overflowY: "auto", padding: 28 },
  space: { display: "flex", flexDirection: "column", gap: 18, maxWidth: 680 },
  spaceTitle: { fontSize: 22, fontWeight: 800, margin: 0 },
  spaceLead: { color: "#94a3b8", fontSize: 14, lineHeight: "21px", margin: 0 },
};

const homeStyles: Record<string, CSSProperties> = {
  cards: { display: "grid", gap: 14, gridTemplateColumns: "repeat(2, 1fr)" },
  card: {
    background: "rgba(15, 26, 44, 0.6)",
    border: "1px solid rgba(6, 182, 212, 0.18)",
    borderRadius: 12,
    boxSizing: "border-box",
    padding: "16px 16px 18px",
  },
  cardHead: {
    color: "#64748b",
    fontSize: 11,
    fontWeight: 800,
    letterSpacing: 1,
    textTransform: "uppercase",
  },
  cardValue: { color: INK, fontSize: 18, fontWeight: 800, marginTop: 8 },
  cardSub: { color: "#94a3b8", fontSize: 12, marginTop: 4 },
};

const loginStyles: Record<string, CSSProperties> = {
  card: {
    alignItems: "center",
    background: "linear-gradient(180deg, rgba(15, 26, 44, 0.92), rgba(8, 16, 30, 0.92))",
    border: `1px solid ${CYAN}`,
    borderRadius: 16,
    boxShadow: "0 32px 110px rgba(0,0,0,0.55), 0 0 52px rgba(6,182,212,0.12)",
    display: "flex",
    flexDirection: "column",
    gap: 14,
    padding: "40px 44px 34px",
    textAlign: "center",
    width: "min(420px, calc(100vw - 48px))",
  },
  title: { fontSize: 30, fontWeight: 800, letterSpacing: 4, margin: "6px 0 0" },
  subtitle: { color: "#94a3b8", fontSize: 14, lineHeight: "21px", margin: 0, maxWidth: 320 },
  button: {
    background: CYAN,
    border: "none",
    borderRadius: 10,
    color: NAVY,
    cursor: "pointer",
    fontSize: 15,
    fontWeight: 800,
    marginTop: 8,
    padding: "12px 22px",
    width: "100%",
  },
  error: { color: "#fca5a5", fontSize: 13, margin: 0 },
  hint: { color: "#64748b", fontSize: 12, margin: "4px 0 0" },
  powered: {
    color: "#475569",
    fontSize: 10,
    fontWeight: 800,
    letterSpacing: 1,
    margin: "12px 0 0",
    textTransform: "uppercase",
  },
};

const overlayStyles: Record<string, CSSProperties> = {
  root: {
    alignItems: "center",
    background: "rgba(4, 9, 20, 0.91)",
    boxSizing: "border-box",
    color: "#e2e8f0",
    display: "flex",
    fontFamily: fontStack,
    inset: 0,
    justifyContent: "center",
    padding: 24,
    position: "fixed",
  },
  card: {
    background: "linear-gradient(180deg, rgba(15, 26, 44, 0.96), rgba(8, 16, 30, 0.96))",
    border: `2px solid ${CYAN}`,
    borderRadius: 8,
    boxShadow: "0 32px 110px rgba(0, 0, 0, 0.58), 0 0 52px rgba(6, 182, 212, 0.14)",
    boxSizing: "border-box",
    minHeight: 360,
    padding: "26px 40px 24px",
    position: "relative",
    textAlign: "center",
    width: "min(560px, calc(100vw - 48px))",
  },
  livePill: {
    alignItems: "center",
    background: "rgba(6, 182, 212, 0.15)",
    border: "1px solid rgba(6, 182, 212, 0.55)",
    borderRadius: 999,
    color: "#e2e8f0",
    display: "inline-flex",
    fontSize: 12,
    fontWeight: 800,
    gap: 9,
    lineHeight: "18px",
    padding: "6px 12px",
    position: "absolute",
    right: 20,
    top: 18,
  },
  title: {
    color: "#f8fafc",
    fontSize: 30,
    fontWeight: 800,
    lineHeight: "38px",
    margin: "10px 0 0",
    overflowWrap: "anywhere",
  },
  subtitle: { color: "#94a3b8", fontSize: 16, lineHeight: "24px", margin: "14px auto 0", maxWidth: 460 },
  footer: { color: "#64748b", fontSize: 12, fontWeight: 800, lineHeight: "18px", marginTop: 24 },
};

const overlayControlStyles: Record<string, CSSProperties> = {
  root: {
    alignItems: "center",
    background: "transparent",
    boxSizing: "border-box",
    display: "flex",
    height: "100vh",
    justifyContent: "center",
    margin: 0,
    overflow: "hidden",
    width: "100vw",
  },
  button: {
    background: "rgba(6, 182, 212, 0.18)",
    border: `1px solid ${CYAN}`,
    borderRadius: 999,
    boxShadow: "0 18px 48px rgba(0, 0, 0, 0.35)",
    color: "#e2e8f0",
    fontSize: 13,
    fontWeight: 800,
    lineHeight: "18px",
    minHeight: 38,
    padding: "9px 20px",
    whiteSpace: "nowrap",
  },
};

const overlayCss = `
  .sonar-ping-scene {
    height: 150px;
    margin: 30px auto 12px;
    position: relative;
    width: 150px;
  }
  .sonar-ping {
    border: 2px solid #22d3ee;
    border-radius: 50%;
    inset: 0;
    margin: auto;
    opacity: 0;
    position: absolute;
    animation: sonar-ping 2.4s infinite cubic-bezier(0.2, 0.6, 0.3, 1);
  }
  .sonar-ping.ring2 { animation-delay: 0.8s; }
  .sonar-ping.ring3 { animation-delay: 1.6s; }
  .sonar-ping-core {
    background: #06b6d4;
    border-radius: 50%;
    box-shadow: 0 0 18px rgba(6, 182, 212, 0.8);
    height: 18px;
    inset: 0;
    margin: auto;
    position: absolute;
    width: 18px;
    animation: sonar-core 1.3s infinite ease-in-out;
  }
  .sonar-live-dot {
    animation: sonar-core 1.25s infinite ease-in-out;
    background: #06b6d4;
    border-radius: 50%;
    display: inline-block;
    height: 9px;
    width: 9px;
  }
  @keyframes sonar-ping {
    0% { height: 18px; width: 18px; opacity: 0.9; }
    100% { height: 150px; width: 150px; opacity: 0; }
  }
  @keyframes sonar-core {
    0%, 100% { opacity: 0.7; transform: scale(0.92); }
    50% { opacity: 1; transform: scale(1.08); }
  }
`;
