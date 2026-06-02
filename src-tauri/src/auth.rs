//! Subunit-Account-Login — OAuth-Browser-Flow über einen 127.0.0.1-Loopback-Callback
//! plus JIT-Token-Refresh. Portiert aus echo/src-tauri/src/auth.rs.
//!
//! Flow: ephemeren localhost-Port binden → Browser zu
//! `auth.subunit.ai/sonar-login?state=<csrf>&port=<port>` öffnen → der Auth-Server
//! leitet auf `http://127.0.0.1:<port>/callback?state&access_token&...` um →
//! `state` verifizieren, Tokens (+ signierten `op`-Claim) speichern, Tab schließen.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use rand::{distributions::Alphanumeric, Rng};
use tauri::{AppHandle, Manager};

use crate::config::AppState;

const AUTH_BASE: &str = "https://auth.subunit.ai";
// 30 min — toleriert langsame E-Mail-Code-Zustellung.
const LOGIN_TIMEOUT_SECS: u64 = 1800;

#[derive(serde::Serialize)]
pub struct AccountState {
    pub logged_in: bool,
    pub email: String,
    pub is_operator: bool,
    pub workspace_id: String,
}

/// Liest den aktuellen Account-Zustand aus dem Config-Store.
#[tauri::command]
pub fn account_state(app: AppHandle) -> AccountState {
    let state = app.state::<AppState>();
    let config = state.config.lock();
    AccountState {
        logged_in: config.is_logged_in(),
        email: config.account_email.clone(),
        is_operator: config.account_is_operator,
        workspace_id: config.subunit_workspace_id.clone(),
    }
}

/// Startet den Login (blockierender Loopback-Flow auf einem Blocking-Thread) und
/// liefert den frischen Account-Zustand zurück.
#[tauri::command]
pub async fn account_login(app: AppHandle) -> Result<AccountState, String> {
    let app_for_login = app.clone();
    tauri::async_runtime::spawn_blocking(move || login(&app_for_login))
        .await
        .map_err(|error| format!("login task join error: {error}"))?
        .map_err(|error| error.to_string())?;
    Ok(account_state(app))
}

/// Meldet den Account ab (löscht Tokens lokal).
#[tauri::command]
pub fn account_logout(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut config = state.config.lock();
    config.clear_account();
    config.save().map_err(|error| error.to_string())
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn random_state() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

/// Öffnet die URL im System-Default-Browser über das Opener-Plugin
/// (`ShellExecuteExW` auf Windows) — die URL geht als EIN String an die Shell.
///
/// NICHT `cmd /C start "" <url>`: dort ist `&` ein Befehlstrenner, der
/// `&port=…` aus der Login-URL abschneidet → der Auth-Server bekommt nur
/// `?state=…`, sieht `!PORT` und zeigt „Diese Seite wurde nicht von Sonar
/// Desktop geöffnet". (Identischer Fix wie echo/src-tauri/src/auth.rs, commit
/// 2aa57a3 „Fix command injection vulnerability in auth browser open".)
fn open_browser(url: &str) {
    if let Err(e) = tauri_plugin_opener::open_url(url.to_string(), None::<&str>) {
        eprintln!("failed to open browser: {e}");
    }
}

/// Blockierend: öffnet den Browser und wartet auf den Loopback-Callback. Liefert
/// die Account-E-Mail (aus dem JWT) oder "Angemeldet".
pub fn login(app: &AppHandle) -> anyhow::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let state = random_state();
    let url = format!("{AUTH_BASE}/sonar-login?state={state}&port={port}");
    open_browser(&url);

    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + Duration::from_secs(LOGIN_TIMEOUT_SECS);

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // Blockierend + bounded lesen, damit ein stiller lokaler Client den Login nicht aufhängt.
                stream.set_nonblocking(false).ok();
                let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                let reqline = read_request_line(&stream).unwrap_or_default();
                let path = reqline.split_whitespace().nth(1).unwrap_or("").to_string();
                let route = path.splitn(2, '?').next().unwrap_or("");

                // Nur die exakte Route; bis zum gültigen Callback (oder Deadline) weiter bedienen
                // (verirrte/gefälschte Requests werden beantwortet + ignoriert, nicht fatal).
                if route != "/callback" {
                    let _ = write_html(&mut stream, "Sonar", "Warte auf Login…");
                    continue;
                }
                let qs = path.splitn(2, '?').nth(1).unwrap_or("");
                let params = query_params(qs);
                if params.get("state").map(String::as_str) != Some(state.as_str()) {
                    let _ = write_html(&mut stream, "Sonar", "Warte auf Login…");
                    continue; // CSRF / stale tab — ignorieren, weiter warten
                }
                let access = params.get("access_token").cloned().unwrap_or_default();
                if access.is_empty() {
                    let _ = write_html(&mut stream, "Sonar", "Warte auf Login…");
                    continue;
                }
                let refresh = params.get("refresh_token").cloned().unwrap_or_default();
                let expires: i32 = params
                    .get("expires_in")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let workspace = params.get("workspace_id").cloned().unwrap_or_default();
                let claims = decode_jwt_claims(&access);
                let email = claims
                    .as_ref()
                    .and_then(|j| {
                        j.get("email")
                            .or_else(|| j.get("sub"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or_default()
                    .to_string();
                let is_operator = claims
                    .as_ref()
                    .and_then(|j| j.get("op").and_then(|v| v.as_bool()))
                    .unwrap_or(false);

                {
                    let st = app.state::<AppState>();
                    let mut c = st.config.lock();
                    c.subunit_access_token = access;
                    c.subunit_refresh_token = refresh;
                    c.subunit_token_expires_in = expires;
                    c.subunit_token_issued_at = now_secs();
                    c.subunit_workspace_id = workspace;
                    c.account_is_operator = is_operator;
                    if !email.is_empty() {
                        c.account_email = email.clone();
                    }
                    let _ = c.save();
                }

                let _ = write_html(
                    &mut stream,
                    "Sonar — angemeldet ✓",
                    "Du kannst dieses Fenster schließen.",
                );
                return Ok(if email.is_empty() {
                    "Angemeldet".to_string()
                } else {
                    email
                });
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    anyhow::bail!("login timed out");
                }
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Frischt den Access-Token auf, wenn er abgelaufen (oder kurz davor) ist. Best-effort.
#[allow(dead_code)]
pub fn ensure_fresh(app: &AppHandle) {
    let (refresh, issued, expires) = {
        let st = app.state::<AppState>();
        let c = st.config.lock();
        (
            c.subunit_refresh_token.clone(),
            c.subunit_token_issued_at,
            c.subunit_token_expires_in,
        )
    };
    if refresh.is_empty() {
        return;
    }
    let now = now_secs();
    // Noch gültig (60s Sicherheitsmarge)?
    if issued > 0.0 && expires > 0 && (now - issued) < (expires as f64 - 60.0) {
        return;
    }
    match do_refresh(&refresh) {
        Ok((access, new_refresh, exp)) => {
            let st = app.state::<AppState>();
            let mut c = st.config.lock();
            c.subunit_access_token = access;
            if !new_refresh.is_empty() {
                c.subunit_refresh_token = new_refresh;
            }
            c.subunit_token_expires_in = exp;
            c.subunit_token_issued_at = now;
            let _ = c.save();
        }
        Err(e) => {
            eprintln!("token refresh failed: {e}");
            // Access-Token ist abgelaufen + Refresh fehlgeschlagen → Access leeren,
            // Refresh-Token für späteren Retry behalten.
            let st = app.state::<AppState>();
            let mut c = st.config.lock();
            c.subunit_access_token.clear();
            c.subunit_token_issued_at = 0.0;
            c.subunit_token_expires_in = 0;
            let _ = c.save();
        }
    }
}

fn do_refresh(refresh_token: &str) -> anyhow::Result<(String, String, i32)> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;
    let resp = client
        .post(format!("{AUTH_BASE}/refresh"))
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()?;
    if !resp.status().is_success() {
        anyhow::bail!("refresh {}", resp.status());
    }
    let j: serde_json::Value = resp.json()?;
    let access = j
        .get("access_token")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if access.is_empty() {
        anyhow::bail!("refresh returned no access token");
    }
    Ok((
        access,
        j.get("refresh_token")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        j.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
    ))
}

// Request-Zeile hart begrenzen (8 KiB) — ein lokaler Client darf nicht unbounded
// senden (Login-DoS, Codex-Finding #6). Read-Timeout/Deadline gibt es zusätzlich.
const MAX_REQUEST_LINE: u64 = 8 * 1024;

fn read_request_line(stream: &TcpStream) -> anyhow::Result<String> {
    let mut reader = BufReader::new(stream.try_clone()?).take(MAX_REQUEST_LINE);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line)
}

fn write_html(stream: &mut TcpStream, title: &str, msg: &str) -> std::io::Result<()> {
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"></head>\
<body style=\"font-family:-apple-system,system-ui,sans-serif;background:#0f172a;color:#e2e8f0;text-align:center;padding-top:90px\">\
<h2 style=\"color:#22d3ee\">{title}</h2><p>{msg}</p></body></html>"
    );
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(resp.as_bytes())
}

fn query_params(qs: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in qs.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        map.insert(percent_decode(k), percent_decode(v));
    }
    map
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
                out.push(b'%');
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Dekodiert die JWT-Payload (base64url, ohne Signatur-Verify — nur zum Auslesen
/// von `email`/`op` für die UI; die Bridge verifiziert das Token serverseitig).
fn decode_jwt_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}
