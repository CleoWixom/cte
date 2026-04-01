use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::StreamExt;
use serde_json::Value;
use std::sync::Arc;

use crate::{
    db::{insert_measurements, upsert_tower},
    normalizer::parse_android_cell,
    state::AppState,
};

pub async fn android_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    tracing::info!("Android WS connected");

    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(text) => {
                if let Ok(json) = serde_json::from_str::<Value>(&text) {
                    if json.get("type").and_then(|v| v.as_str()) == Some("measurement") {
                        if let Some(cells) = json.get("cells").and_then(|v| v.as_array()) {
                            let mut imported = 0u64;
                            for cell in cells {
                                if let Some(norm) = parse_android_cell(cell) {
                                    if let Ok(tower_id) = upsert_tower(
                                        &state.db,
                                        &norm.radio,
                                        norm.mcc,
                                        norm.mnc,
                                        norm.lac,
                                        norm.cid,
                                        norm.lat,
                                        norm.lon,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        let batch = [(
                                            tower_id,
                                            norm.lat,
                                            norm.lon,
                                            norm.signal_dbm,
                                            norm.raw_signal,
                                            "android",
                                            norm.radio.as_str(),
                                        )];
                                        imported += insert_measurements(&state.db, &batch)
                                            .await
                                            .unwrap_or(0);
                                    }
                                }
                            }

                            let ack = serde_json::json!({
                                "type": "ack",
                                "imported": imported,
                            });
                            let _ = socket.send(Message::Text(ack.to_string())).await;
                        }
                    }
                }
            }
            Message::Ping(p) => {
                let _ = socket.send(Message::Pong(p)).await;
            }
            Message::Close(_) => {
                tracing::info!("Android WS disconnected");
                break;
            }
            _ => {}
        }
    }
}
