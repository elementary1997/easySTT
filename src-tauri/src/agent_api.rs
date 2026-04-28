//! Agent Hub integration for easySTT.
//!
//! Implements the minimal HTTP + WebSocket contract from
//! <https://github.com/elementary1997/agent-hub/blob/main/PROTOCOL.md> v0.1
//! so easySTT can be discovered and supervised by the hub without changing
//! how it works as a standalone app.
//!
//! On startup we:
//! 1. Pick a free port (or use the one stored in settings),
//! 2. Spin up a small axum server bound to `127.0.0.1`,
//! 3. Drop a manifest JSON into `~/.config/agent-hub/agents/easystt.json`
//!    (or the platform equivalent) so the hub can find us.

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Listener, Manager};
use tokio::sync::broadcast;

const AGENT_ID: &str = "easystt";
const PORT_FALLBACK_RANGE: std::ops::Range<u16> = 8731..8800;

/// Shared state held inside the axum server.
#[derive(Clone)]
struct ApiState {
    app: AppHandle,
    started: Arc<Instant>,
    events: broadcast::Sender<Value>,
}

/// Spawns the API server on a Tauri-managed Tokio task and writes the manifest.
/// Failures are logged but don't crash easySTT — the standalone app still works.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run(app).await {
            eprintln!("[agent_api] server error: {e}");
        }
    });
}

async fn run(app: AppHandle) -> anyhow::Result<()> {
    let listener = bind_free_port().await?;
    let port = listener.local_addr()?.port();
    let endpoint = format!("http://127.0.0.1:{port}");
    write_manifest(&endpoint)?;
    println!("[agent_api] easySTT manifest published, listening on {endpoint}");

    let (tx, _rx) = broadcast::channel::<Value>(64);
    bridge_tauri_events(&app, tx.clone());

    let state = ApiState {
        app: app.clone(),
        started: Arc::new(Instant::now()),
        events: tx,
    };

    let app_router = Router::new()
        .route("/status", get(status))
        .route("/open-native-ui", post(open_native_ui))
        .route("/quit", post(quit_agent))
        .route("/events", get(events_ws))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state);

    axum::serve(listener, app_router).await?;
    Ok(())
}

/// Tries our preferred port first, then walks a small range. The chosen port
/// ends up in the manifest so the hub knows where to connect.
async fn bind_free_port() -> anyhow::Result<tokio::net::TcpListener> {
    for port in PORT_FALLBACK_RANGE {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        // probe with std listener first — fast fail without async overhead
        if StdTcpListener::bind(addr).is_ok() {
            return Ok(tokio::net::TcpListener::bind(addr).await?);
        }
    }
    Err(anyhow::anyhow!("no free port in {:?}", PORT_FALLBACK_RANGE))
}

// ─── Endpoints ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct StatusResponse {
    alive: bool,
    busy: bool,
    version: &'static str,
    uptime_sec: u64,
}

async fn status(State(s): State<ApiState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        alive: true,
        // We don't track a global "busy" flag yet — hub will receive a more
        // accurate signal via the agent_busy events bridged from Tauri.
        busy: false,
        version: env!("CARGO_PKG_VERSION"),
        uptime_sec: s.started.elapsed().as_secs(),
    })
}

async fn open_native_ui(State(s): State<ApiState>) -> impl IntoResponse {
    if let Some(w) = s.app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return (StatusCode::OK, Json(json!({ "ok": true }))).into_response();
    }
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": { "code": "no_window", "message": "settings window unavailable" } })),
    )
        .into_response()
}

#[derive(Deserialize, Default)]
struct QuitBody {
    #[serde(default)]
    grace_ms: Option<u64>,
}

async fn quit_agent(
    State(s): State<ApiState>,
    body: Option<Json<QuitBody>>,
) -> impl IntoResponse {
    let grace = body.and_then(|b| b.0.grace_ms).unwrap_or(0);
    let app = s.app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(grace)).await;
        app.exit(0);
    });
    (StatusCode::ACCEPTED, Json(json!({ "ok": true })))
}

async fn events_ws(ws: WebSocketUpgrade, State(s): State<ApiState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, s.events.subscribe()))
}

async fn handle_socket(mut ws: WebSocket, mut rx: broadcast::Receiver<Value>) {
    // Hello-frame so the hub knows the channel is live even before any
    // domain event happens.
    let hello = json!({ "type": "hello", "data": { "agent": AGENT_ID } });
    if ws.send(Message::Text(hello.to_string())).await.is_err() {
        return;
    }
    while let Ok(ev) = rx.recv().await {
        if ws.send(Message::Text(ev.to_string())).await.is_err() {
            break;
        }
    }
}

// ─── Tauri ↔ event bus bridge ────────────────────────────────────────────────

/// Subscribe to the existing easySTT Tauri events and re-emit them on the
/// broadcast channel in the protocol's shape. We deliberately keep the schema
/// flat (`type` + `data`) so the hub doesn't have to know about easySTT's
/// internals.
fn bridge_tauri_events(app: &AppHandle, tx: broadcast::Sender<Value>) {
    let pairs: &[(&str, &str)] = &[
        ("ptt-pressed", "agent_busy"),
        ("ptt-released", "agent_busy"),
        ("transcription-done", "transcript_done"),
        ("transcription-error", "error"),
        ("transcription-cancelled", "transcript_cancelled"),
    ];
    for (tauri_event, protocol_type) in pairs {
        let tx = tx.clone();
        let proto = (*protocol_type).to_string();
        let src = (*tauri_event).to_string();
        app.listen(*tauri_event, move |event| {
            let payload = serde_json::from_str::<Value>(event.payload())
                .unwrap_or_else(|_| Value::String(event.payload().to_string()));
            // Special-case agent_busy: derive busy=true/false from the event name.
            let data = if proto == "agent_busy" {
                json!({ "busy": src == "ptt-pressed", "reason": src })
            } else {
                json!({ "payload": payload })
            };
            let _ = tx.send(json!({ "type": proto, "data": data }));
        });
    }
    // Heartbeat — keeps the channel from idling out behind some proxies.
    let tx = tx.clone();
    tauri::async_runtime::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(30));
        loop {
            t.tick().await;
            let _ = tx.send(json!({ "type": "heartbeat", "data": {} }));
        }
    });
}

// ─── Manifest writer ────────────────────────────────────────────────────────

fn manifest_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("agent-hub")
        .join("agents")
}

fn write_manifest(endpoint: &str) -> anyhow::Result<()> {
    let dir = manifest_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{AGENT_ID}.json"));
    let manifest = json!({
        "id": AGENT_ID,
        "name": "easySTT",
        "version": env!("CARGO_PKG_VERSION"),
        "kind": "utility",
        "endpoint": endpoint,
        "lifecycle": "standalone",
        "tagline": "Voice-to-text injection",
        "tags": ["productivity", "input"],
        "accent": "#4c84ff",
        "protocol": "0.1"
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&manifest)?)?;
    Ok(())
}
