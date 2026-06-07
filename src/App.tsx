import {
  useCallback,
  useEffect,
  useRef,
  useState,
  memo,
  type CSSProperties,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { BridgeCard, type BridgeStatus } from "./BridgeCard";
import { ForgeAccessCard, type ConsentState } from "./ForgeAccessCard";
import { CrystalOverlay } from "./CrystalOverlay";
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

type UpdateInfo = {
  version: string;
  notes: string;
};

type AccountState = {
  logged_in: boolean;
  email: string;
  is_operator: boolean;
  workspace_id: string;
};

type SpaceId = "home" | "forge" | "trace";

/**
 * Transparente Overlay-Fenster (overlay + overlayControl) laden dieselbe
 * index.html wie das Hauptfenster. Ohne CSS-Reset bleibt der Browser-Default
 * `body { margin: 8px }` aktiv → zusammen mit width/height:100% ragt der Inhalt
 * über den Viewport → horizontale + vertikale Scrollbar (die den Ausblenden-
 * Button verdeckt haben). Dieser Hook setzt html/body/#root NUR in den
 * Overlay-Fenstern auf randlos/transparent/overflow-hidden — das Hauptfenster
 * (eigener Webview) bleibt unberührt und weiterhin scrollbar.
 */
function useTransparentChrome() {
  useEffect(() => {
    const root = document.getElementById("root");
    const targets = [document.documentElement, document.body, root].filter(
      (el): el is HTMLElement => el != null,
    );
    const previous = targets.map((el) => el.getAttribute("style") ?? "");
    for (const el of targets) {
      el.style.margin = "0";
      el.style.padding = "0";
      el.style.width = "100%";
      el.style.height = "100%";
      el.style.background = "transparent";
      el.style.overflow = "hidden";
    }
    return () => {
      targets.forEach((el, index) => {
        if (previous[index]) el.setAttribute("style", previous[index]);
        else el.removeAttribute("style");
      });
    };
  }, []);
}

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
  const [pairing, setPairing] = useState(false);
  // Auto-Re-Pair nur einmal pro Session versuchen (sonst bei jedem Poll) — Fallback ist der Button.
  const autoPairAttempted = useRef(false);

  // Update: Version anzeigen, Auto-Suche (Event vom Backend) + manuelle Suche + Ein-Klick-Install.
  const [appVersion, setAppVersion] = useState("");
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateInstalling, setUpdateInstalling] = useState(false);
  const [updateNote, setUpdateNote] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => undefined);
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    // Backend prüft beim Start + alle 6 h und meldet ein gefundenes Update hierüber.
    listen<UpdateInfo>("update://available", (event) => {
      setUpdate(event.payload);
      setUpdateNote(null);
    }).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  async function checkForUpdates() {
    setUpdateChecking(true);
    setUpdateNote(null);
    try {
      const result = await invoke<UpdateInfo | null>("update_check");
      if (result) {
        setUpdate(result);
      } else {
        setUpdateNote("Du bist auf dem neuesten Stand.");
      }
    } catch (caught) {
      setUpdateNote(`Update-Suche fehlgeschlagen: ${formatError(caught)}`);
    } finally {
      setUpdateChecking(false);
    }
  }

  async function installUpdate() {
    setUpdateInstalling(true);
    setUpdateNote("Update wird installiert — Sonar startet gleich neu…");
    try {
      // Bei Erfolg startet die App neu → der Code danach wird i. d. R. nicht mehr erreicht.
      await invoke("update_install");
    } catch (caught) {
      setUpdateInstalling(false);
      setUpdateNote(`Installation fehlgeschlagen: ${formatError(caught)}`);
    }
  }

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
          // Auto-Re-Pair: Bridge läuft, ist aber ungepairt (war beim Login offline/Zombie) → einmal
          // pro Session automatisch koppeln. Schlägt es fehl, bleibt der manuelle „Koppeln"-Button.
          if (next.online && next.paired === false && !autoPairAttempted.current) {
            autoPairAttempted.current = true;
            invoke("bridge_pair").catch((e) => console.error("auto bridge_pair:", e));
          }
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
    // Lokales Abmelden läuft IMMER durch (finally) — sonst sperrt man sich aus (kein Abmelden →
    // kein erneutes An-/Koppeln). Die Bridge-Entkopplung passiert best-effort in account_logout.
    try {
      await invoke("account_logout");
    } catch (caught) {
      console.error("account_logout:", caught);
    } finally {
      onAccountChange();
    }
  }

  async function pair() {
    // Manuelles Koppeln: übergibt das gespeicherte Token erneut an die Bridge (für „eingeloggt,
    // aber ungepairt"). Der Status-Poll aktualisiert danach den Indikator.
    setPairing(true);
    try {
      await invoke("bridge_pair");
    } catch (caught) {
      console.error("bridge_pair:", caught);
      window.alert("Koppeln fehlgeschlagen — bitte sicherstellen, dass du angemeldet bist, und erneut versuchen.");
    } finally {
      setPairing(false);
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
          {status?.online && status.paired === false ? (
            <button onClick={pair} disabled={pairing} style={shellStyles.pairButton} type="button">
              {pairing ? "Koppelt…" : "Koppeln"}
            </button>
          ) : null}
          <button onClick={logout} style={shellStyles.logout} type="button">
            Abmelden
          </button>
          <div style={shellStyles.updateBox}>
            <span style={shellStyles.versionText}>Version {appVersion || "—"}</span>
            <button
              onClick={checkForUpdates}
              disabled={updateChecking}
              style={shellStyles.updateCheck}
              type="button"
            >
              {updateChecking ? "Suche…" : "Nach Updates suchen"}
            </button>
            {updateNote ? <span style={shellStyles.updateNote}>{updateNote}</span> : null}
          </div>
        </div>
      </nav>

      <section style={shellStyles.content}>
        {update ? (
          <div style={updateStyles.banner}>
            <div style={updateStyles.bannerText}>
              <span style={updateStyles.bannerTitle}>Update verfügbar — Version {update.version}</span>
              <span style={updateStyles.bannerSub}>
                Ein Klick lädt die neue Version und startet Sonar automatisch neu.
              </span>
            </div>
            <button
              style={{ ...updateStyles.bannerButton, opacity: updateInstalling ? 0.6 : 1 }}
              onClick={installUpdate}
              disabled={updateInstalling}
              type="button"
            >
              {updateInstalling ? "Installiere…" : "Jetzt installieren"}
            </button>
          </div>
        ) : null}
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
  const [helpModalOpen, setHelpModalOpen] = useState(false);
  const [helpDraft, setHelpDraft] = useState("");
  // Forge-Einstellung: „u1 arbeitet"-Overlay anzeigen (default an, persistiert im Config-Dir).
  const [overlayEnabled, setOverlayEnabled] = useState<boolean | null>(null);

  useEffect(() => {
    invoke<boolean>("forge_overlay_enabled")
      .then(setOverlayEnabled)
      .catch(() => setOverlayEnabled(true));
  }, []);

  async function toggleOverlay() {
    const next = !(overlayEnabled ?? true);
    setOverlayEnabled(next); // optimistisch
    try {
      await invoke("set_forge_overlay_enabled", { enabled: next });
    } catch (caught) {
      setOverlayEnabled(!next); // bei Fehler zurückdrehen
      console.error("set_forge_overlay_enabled:", caught);
    }
  }

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

  function openHelp() {
    setHelpMessage(null);
    setHelpError(null);
    setHelpDraft("");
    setHelpModalOpen(true);
  }

  async function submitHelp() {
    setHelpActionPending(true);
    setHelpError(null);
    try {
      const result = await invoke<HelpRequestResult>("help_request", { message: helpDraft.trim() });
      setHelpMessage(result.message);
      setHelpModalOpen(false);
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
        onHelpRequest={openHelp}
      />
      <div style={forgeSettingsStyles.card}>
        <div style={forgeSettingsStyles.row}>
          <div style={forgeSettingsStyles.text}>
            <div style={forgeSettingsStyles.title}>„u1 arbeitet"-Overlay</div>
            <div style={forgeSettingsStyles.desc}>
              Vollbild-Hinweis mit rotierendem Kristall während des Fernzugriffs. Du kannst es
              jederzeit über „Ausblenden" wegklicken — hier schaltest du aus, ob es überhaupt erscheint.
            </div>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={overlayEnabled ?? true}
            onClick={toggleOverlay}
            disabled={overlayEnabled === null}
            style={{
              ...forgeSettingsStyles.toggle,
              ...((overlayEnabled ?? true) ? forgeSettingsStyles.toggleOn : forgeSettingsStyles.toggleOff),
              opacity: overlayEnabled === null ? 0.5 : 1,
            }}
            title={(overlayEnabled ?? true) ? "Overlay ist an" : "Overlay ist aus"}
          >
            <span
              style={{
                ...forgeSettingsStyles.knob,
                ...((overlayEnabled ?? true) ? forgeSettingsStyles.knobOn : forgeSettingsStyles.knobOff),
              }}
            />
          </button>
        </div>
      </div>
      {helpModalOpen ? (
        <div style={helpModalStyles.backdrop} onClick={() => !helpActionPending && setHelpModalOpen(false)}>
          <div style={helpModalStyles.card} onClick={(e) => e.stopPropagation()}>
            <h2 style={helpModalStyles.title}>Hilfe anfordern</h2>
            <p style={helpModalStyles.lead}>
              Was ist das Problem? Beschreib es kurz — das Subunit-Team wird sofort benachrichtigt.
            </p>
            <textarea
              style={helpModalStyles.textarea}
              value={helpDraft}
              onChange={(e) => setHelpDraft(e.target.value)}
              placeholder="z. B. „Outlook startet seit heute früh nicht mehr…“"
              rows={5}
              autoFocus
            />
            {helpError ? <p style={helpModalStyles.error}>{helpError}</p> : null}
            <div style={helpModalStyles.actions}>
              <button
                style={helpModalStyles.cancel}
                onClick={() => setHelpModalOpen(false)}
                disabled={helpActionPending}
                type="button"
              >
                Abbrechen
              </button>
              <button
                style={helpModalStyles.submit}
                onClick={submitHelp}
                disabled={helpActionPending}
                type="button"
              >
                {helpActionPending ? "Sende…" : "Senden"}
              </button>
            </div>
          </div>
        </div>
      ) : null}
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
  useTransparentChrome();

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

        <div style={overlayStyles.crystalScene} aria-hidden="true">
          <CrystalOverlay />
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
  useTransparentChrome();

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

const helpModalStyles: Record<string, CSSProperties> = {
  backdrop: {
    position: "fixed",
    inset: 0,
    background: "rgba(3, 11, 24, 0.7)",
    backdropFilter: "blur(4px)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    zIndex: 1000,
  },
  card: {
    width: "min(440px, 90vw)",
    background: "#0f172a",
    border: "1px solid rgba(6, 182, 212, 0.3)",
    borderRadius: 14,
    padding: 22,
    boxShadow: "0 20px 60px rgba(0,0,0,0.5)",
  },
  title: { margin: "0 0 6px", fontSize: 18, fontWeight: 700, color: "#e2e8f0" },
  lead: { margin: "0 0 14px", fontSize: 13, color: "#94a3b8", lineHeight: 1.4 },
  textarea: {
    width: "100%",
    boxSizing: "border-box",
    background: "#030b18",
    border: "1px solid rgba(148, 163, 184, 0.3)",
    borderRadius: 8,
    color: "#e2e8f0",
    fontSize: 14,
    padding: "10px 12px",
    resize: "vertical",
    fontFamily: "inherit",
  },
  error: { margin: "10px 0 0", fontSize: 13, color: "#f87171" },
  actions: { display: "flex", justifyContent: "flex-end", gap: 10, marginTop: 16 },
  cancel: {
    background: "transparent",
    border: "1px solid rgba(148, 163, 184, 0.3)",
    borderRadius: 8,
    color: "#94a3b8",
    cursor: "pointer",
    fontSize: 13,
    fontWeight: 600,
    padding: "8px 16px",
  },
  submit: {
    background: "#06b6d4",
    border: "none",
    borderRadius: 8,
    color: "#03121f",
    cursor: "pointer",
    fontSize: 13,
    fontWeight: 700,
    padding: "8px 18px",
  },
};

const forgeSettingsStyles: Record<string, CSSProperties> = {
  card: {
    background: "rgba(15, 26, 44, 0.6)",
    border: "1px solid rgba(6, 182, 212, 0.18)",
    borderRadius: 12,
    boxSizing: "border-box",
    padding: "16px 18px",
  },
  row: { alignItems: "center", display: "flex", gap: 16, justifyContent: "space-between" },
  text: { display: "flex", flexDirection: "column", gap: 3 },
  title: { color: INK, fontSize: 14, fontWeight: 700 },
  desc: { color: "#94a3b8", fontSize: 12, lineHeight: "17px", maxWidth: 460 },
  toggle: {
    border: "none",
    borderRadius: 999,
    cursor: "pointer",
    flexShrink: 0,
    height: 26,
    padding: 0,
    position: "relative",
    transition: "background 0.18s ease",
    width: 46,
  },
  toggleOn: { background: "#06b6d4" },
  toggleOff: { background: "rgba(148, 163, 184, 0.4)" },
  knob: {
    background: "#ffffff",
    borderRadius: "50%",
    height: 20,
    position: "absolute",
    top: 3,
    transition: "transform 0.18s ease",
    width: 20,
    boxShadow: "0 1px 3px rgba(0,0,0,0.4)",
  },
  knobOn: { transform: "translateX(23px)" },
  knobOff: { transform: "translateX(3px)" },
};

const updateStyles: Record<string, CSSProperties> = {
  banner: {
    alignItems: "center",
    background: "linear-gradient(100deg, rgba(6, 182, 212, 0.16), rgba(6, 182, 212, 0.06))",
    border: "1px solid rgba(6, 182, 212, 0.45)",
    borderRadius: 12,
    boxShadow: "0 8px 30px rgba(6, 182, 212, 0.12)",
    display: "flex",
    gap: 16,
    justifyContent: "space-between",
    marginBottom: 20,
    padding: "14px 18px",
  },
  bannerText: { display: "flex", flexDirection: "column", gap: 2 },
  bannerTitle: { color: "#ecfeff", fontSize: 14, fontWeight: 700 },
  bannerSub: { color: "#94a3b8", fontSize: 12 },
  bannerButton: {
    background: "#06b6d4",
    border: "none",
    borderRadius: 9,
    color: "#03121f",
    cursor: "pointer",
    flexShrink: 0,
    fontSize: 13,
    fontWeight: 800,
    padding: "9px 18px",
    whiteSpace: "nowrap",
  },
};

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
  pairButton: {
    background: "rgba(6, 182, 212, 0.15)",
    border: "1px solid rgba(6, 182, 212, 0.5)",
    borderRadius: 8,
    color: "#06b6d4",
    cursor: "pointer",
    fontSize: 12,
    fontWeight: 700,
    padding: "7px 10px",
    marginBottom: 6,
  },
  updateBox: {
    borderTop: "1px solid rgba(148, 163, 184, 0.12)",
    display: "flex",
    flexDirection: "column",
    gap: 6,
    marginTop: 4,
    paddingTop: 12,
  },
  versionText: { color: "#64748b", fontSize: 11, fontWeight: 600 },
  updateCheck: {
    background: "transparent",
    border: "none",
    color: "#64748b",
    cursor: "pointer",
    fontSize: 11,
    fontWeight: 600,
    padding: 0,
    textAlign: "left",
    textDecoration: "underline",
    textUnderlineOffset: 2,
  },
  updateNote: { color: "#94a3b8", fontSize: 10, lineHeight: "14px" },
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
  crystalScene: {
    width: 200,
    height: 200,
    margin: "0 auto 8px",
  },
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
    inset: 0,
    justifyContent: "center",
    margin: 0,
    overflow: "hidden",
    position: "fixed",
  },
  button: {
    background: CYAN,
    border: "1px solid rgba(255,255,255,0.28)",
    borderRadius: 999,
    boxShadow: "0 4px 14px rgba(0, 0, 0, 0.4), 0 0 0 2px rgba(6,182,212,0.22)",
    color: "#03121f",
    cursor: "pointer",
    fontSize: 13,
    fontWeight: 800,
    lineHeight: "18px",
    minHeight: 36,
    padding: "8px 18px",
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
