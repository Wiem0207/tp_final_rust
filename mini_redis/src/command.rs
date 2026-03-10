use serde::{Deserialize, Serialize};
use crate::Store;

#[derive(Debug, Deserialize)]
pub struct Request {
    pub cmd: String,
    pub key: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Response {
    Ok      { status: &'static str },
    OkValue { status: &'static str, value: Option<String> },
    OkCount { status: &'static str, count: u32 },
    OkKeys  { status: &'static str, keys: Vec<String> },
    Error   { status: &'static str, message: String },
}

pub fn ok() -> Response {
    Response::Ok { status: "ok" }
}

pub fn error(msg: &str) -> Response {
    Response::Error { status: "error", message: msg.to_string() }
}

pub async fn handle_command(raw: &str, store: &Store) -> Response {
    let req: Request = match serde_json::from_str(raw) {
        Ok(r) => r,
        Err(_) => return error("invalid json"),
    };

    match req.cmd.to_uppercase().as_str() {
        "PING" => ok(),
        "SET"  => cmd_set(req, store).await,
        "GET"  => cmd_get(req, store).await,
        "DEL"  => cmd_del(req, store).await,
        "KEYS" => cmd_keys(store).await,
        _      => error("unknown command"),
    }
}

async fn cmd_set(req: Request, store: &Store) -> Response {
    let key = match req.key { Some(k) => k, None => return error("missing key") };
    let value = match req.value { Some(v) => v, None => return error("missing value") };
    store.lock().await.insert(key, value);
    ok()
}

async fn cmd_get(req: Request, store: &Store) -> Response {
    let key = match req.key { Some(k) => k, None => return error("missing key") };
    let value = store.lock().await.get(&key).cloned();
    Response::OkValue { status: "ok", value }
}

async fn cmd_del(req: Request, store: &Store) -> Response {
    let key = match req.key { Some(k) => k, None => return error("missing key") };
    let existed = store.lock().await.remove(&key).is_some();
    Response::OkCount { status: "ok", count: if existed { 1 } else { 0 } }
}

async fn cmd_keys(store: &Store) -> Response {
    let keys = store.lock().await.keys().cloned().collect();
    Response::OkKeys { status: "ok", keys }
}