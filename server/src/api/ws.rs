//! `GET /ws`: every `ServerEvent` as a JSON text frame.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use tokio::sync::broadcast::{error::RecvError, Receiver};

use super::AppState;
use crate::events::ServerEvent;

pub async fn handler(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    // Subscribed before the 101 goes out so a client that connects and then
    // immediately starts an import cannot miss the first frames.
    let rx = state.events.subscribe();
    upgrade.on_upgrade(move |socket| pump(socket, rx))
}

async fn pump(mut socket: WebSocket, mut rx: Receiver<ServerEvent>) {
    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Ok(event) => {
                    let Ok(text) = serde_json::to_string(&event) else { continue };
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                // A client too slow to keep up skips frames instead of
                // stalling the producer; progress frames are disposable.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_) | Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            },
        }
    }
}
