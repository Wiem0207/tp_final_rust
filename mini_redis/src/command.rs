use serde::{Deserialize, Serialize};
use crate::Store;
use std::time::Instant;

// Vous pouvez stocker l'instant d'expiration avec la valeur :
pub struct Entry {
    value: String,
    pub expires_at: Option<Instant>,
}

#[derive(Debug, Deserialize)]
pub struct Request {
    pub cmd: String,
    pub key: Option<String>,
    pub value: Option<String>,
    pub seconds: Option<u64>,  
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Response {
    Ok      { status: &'static str },
    OkValue { status: &'static str, value: Option<String> },
    OkCount { status: &'static str, count: u32 },
    OkTtl   { status: &'static str, ttl: i64 }, 
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
        "EXPIRE" => cmd_expire(req, store).await,
        "TTL"    => cmd_ttl(req, store).await,
        _      => error("unknown command"),
    }
}

async fn cmd_set(req: Request, store: &Store) -> Response {
    let key = match req.key { Some(k) => k, None => return error("missing key") };
    let value = match req.value { Some(v) => v, None => return error("missing value") };
    store.lock().await.insert(key, Entry { value, expires_at: None });
    ok()
}
// we need to verify that the key did not expire 
async fn cmd_get(req: Request, store: &Store) -> Response {
    let key = match req.key { Some(k) => k, None => return error("missing key") };
    let map = store.lock().await;
    let now = Instant::now();
    let value = match map.get(&key) {
        Some(e) if e.expires_at.map(|t| t > now) != Some(false) => Some(e.value.clone()),
        _ => None,
    };
    Response::OkValue { status: "ok", value }
}

async fn cmd_del(req: Request, store: &Store) -> Response {
    let key = match req.key { Some(k) => k, None => return error("missing key") };
    let existed = store.lock().await.remove(&key).is_some();
    Response::OkCount { status: "ok", count: if existed { 1 } else { 0 } }
}

async fn cmd_keys(store: &Store) -> Response {
    let map = store.lock().await;
    let now = Instant::now();
    let mut keys = Vec::new();
    for (k, e) in map.iter() {
        let expired = e.expires_at.map(|t| t <= now) == Some(true);
        if !expired {
            keys.push(k.clone());
        }
    }
    Response::OkKeys { status: "ok", keys }
}
async fn cmd_expire(req: Request, store: &Store) -> Response {
    let key = match req.key { Some(k) => k, None => return error("missing key") };
    let secs = match req.seconds { Some(s) => s, None => return error("missing seconds") };
    let mut map = store.lock().await;
    match map.get_mut(&key) {
        None => error("key not found"),
        Some(e) => {
            e.expires_at = Some(Instant::now() + std::time::Duration::from_secs(secs));
            ok()
        }
    }
}

async fn cmd_ttl(req: Request, store: &Store) -> Response {
    let key = match req.key { Some(k) => k, None => return error("missing key") };
    match store.lock().await.get(&key) {
        None => Response::OkTtl { status: "ok", ttl: -2 },
        Some(e) => match e.expires_at {
            None => Response::OkTtl { status: "ok", ttl: -1 },
            Some(t) => Response::OkTtl { status: "ok", ttl: t.duration_since(Instant::now()).as_secs() as i64 },
        }
    }
}