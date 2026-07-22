//! `/ws` — live event stream (§10, §12 M3).
//!
//! Subscribes each client to the edge's broadcast bus and forwards every
//! [`WsEvent`](mes_client::ws::WsEvent) as a JSON text frame. Kiosk and
//! supervisor apps render these (§11). Client→server messages are ignored in
//! v1 (the kiosk chat panel logs free text but sends nothing to a model, §11).

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;

use crate::http::AppState;

/// Upgrade the connection and stream events to the client.
pub async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    let rx = state.events.subscribe();
    ws.on_upgrade(move |socket| forward(socket, rx))
}

async fn forward(
    mut socket: WebSocket,
    mut rx: tokio::sync::broadcast::Receiver<mes_client::ws::WsEvent>,
) {
    loop {
        match rx.recv().await {
            Ok(event) => {
                let Ok(text) = serde_json::to_string(&event) else {
                    continue;
                };
                if socket.send(Message::Text(text)).await.is_err() {
                    break; // client went away
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                // Slow client missed events; keep going with the newest.
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}
