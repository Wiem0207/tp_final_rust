use crate::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

// Vous pouvez stocker l'instant d'expiration avec la valeur :
pub struct Entry {
    pub value: String,
    pub expires_at: Option<Instant>,
}

// format des requêtes : objet JSON avec "cmd" obligatoire
// les autres champs sont optionnels selon la commande

#[derive(Debug, Deserialize)]
pub struct Request {
    pub cmd: String,
    pub key: Option<String>,
    pub value: Option<String>,
    pub seconds: Option<u64>,
}
// format des réponses : toujours un champ "status" ("ok" ou "error")
// #[serde(untagged)] pour serialiser sans le nom du variant
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Response {
    Ok {
        status: &'static str,
    },
    OkValue {
        status: &'static str,
        value: Option<String>,
    },
    OkCount {
        status: &'static str,
        count: u32,
    },
    OkTtl {
        status: &'static str,
        ttl: i64,
    },
    OkKeys {
        status: &'static str,
        keys: Vec<String>,
    },
    OkInt {
        status: &'static str,
        value: i64,
    },
    Error {
        status: &'static str,
        message: String,
    },
}

pub async fn handle_command(raw: &str, store: &Store) -> Response {
    let req: Request = match serde_json::from_str(raw) {
        Ok(r) => r,
        Err(_) => {
            return Response::Error {
                status: "error",
                message: "invalid json".to_string(),
            }
        }
    };

    match req.cmd.to_uppercase().as_str() {
        "PING" => Response::Ok { status: "ok" },
        "SET" => cmd_set(req, store).await,
        "GET" => cmd_get(req, store).await,
        "DEL" => cmd_del(req, store).await,
        "KEYS" => cmd_keys(store).await,
        "EXPIRE" => cmd_expire(req, store).await,
        "TTL" => cmd_ttl(req, store).await,
        "INCR" => cmd_incr_decr(req, store, 1).await,
        "DECR" => cmd_incr_decr(req, store, -1).await,
        "SAVE" => cmd_save(store).await,
        _ => Response::Error {
            status: "error",
            message: "unknown command".to_string(),
        },
    }
}
// SET : stocke une paire clé/valeur
// si la clé existe déjà, la valeur est écrasée

async fn cmd_set(req: Request, store: &Store) -> Response {
    let key = match req.key {
        Some(k) => k,
        None => {
            return Response::Error {
                status: "error",
                message: "missing key".to_string(),
            }
        }
    };
    let value = match req.value {
        Some(v) => v,
        None => {
            return Response::Error {
                status: "error",
                message: "missing value".to_string(),
            }
        }
    };
    store.lock().await.insert(
        key,
        Entry {
            value,
            expires_at: None,
        },
    );
    Response::Ok { status: "ok" }
}
// we need to verify that the key did not expire
async fn cmd_get(req: Request, store: &Store) -> Response {
    let key = match req.key {
        Some(k) => k,
        None => {
            return Response::Error {
                status: "error",
                message: "missing key".to_string(),
            }
        }
    };
    let map = store.lock().await;
    let now = Instant::now();
    let value = match map.get(&key) {
        Some(e) if e.expires_at.map(|t| t > now) != Some(false) => Some(e.value.clone()),
        _ => None,
    };
    Response::OkValue {
        status: "ok",
        value,
    }
}
// supprime le key si il existe
async fn cmd_del(req: Request, store: &Store) -> Response {
    let key = match req.key {
        Some(k) => k,
        None => {
            return Response::Error {
                status: "error",
                message: "missing key".to_string(),
            }
        }
    };
    let existed = store.lock().await.remove(&key).is_some();
    Response::OkCount {
        status: "ok",
        count: if existed { 1 } else { 0 },
    }
}
// retournes tous les keys non expired qui sonrt dans le store
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

// EXPIRE : associe un délai d'expiration en secondes à une clé existante

async fn cmd_expire(req: Request, store: &Store) -> Response {
    let key = match req.key {
        Some(k) => k,
        None => {
            return Response::Error {
                status: "error",
                message: "missing key".to_string(),
            }
        }
    };
    let secs = match req.seconds {
        Some(s) => s,
        None => {
            return Response::Error {
                status: "error",
                message: "missing seconds".to_string(),
            }
        }
    };
    let mut map = store.lock().await;
    match map.get_mut(&key) {
        None => Response::Error {
            status: "error",
            message: "key not found".to_string(),
        },
        Some(e) => {
            e.expires_at = Some(Instant::now() + std::time::Duration::from_secs(secs));
            Response::Ok { status: "ok" }
        }
    }
}
// retourne le temps restant avant expiration

async fn cmd_ttl(req: Request, store: &Store) -> Response {
    let key = match req.key {
        Some(k) => k,
        None => {
            return Response::Error {
                status: "error",
                message: "missing key".to_string(),
            }
        }
    };
    match store.lock().await.get(&key) {
        None => Response::OkTtl {
            status: "ok",
            ttl: -2,
        },
        Some(e) => match e.expires_at {
            None => Response::OkTtl {
                status: "ok",
                ttl: -1,
            },
            Some(t) => Response::OkTtl {
                status: "ok",
                ttl: t.duration_since(Instant::now()).as_secs() as i64,
            },
        },
    }
}
// - `INCR` : incrémente la valeur associée à la clé (interprétée comme un entier).
//   Si la clé n'existe pas, elle est créée avec la valeur `1` (resp. `-1` pour `DECR`).
// - La réponse contient la nouvelle valeur : `{"status": "ok", "value": 42}`
// - Si la valeur n'est pas un entier valide → `{"status": "error", "message": "not an integer"}

async fn cmd_incr_decr(req: Request, store: &Store, delta: i64) -> Response {
    let key = match req.key {
        Some(k) => k,
        None => {
            return Response::Error {
                status: "error",
                message: "missing key".to_string(),
            }
        }
    };
    let mut map = store.lock().await;
    let current = match map.get(&key) {
        None => 0,
        Some(e) => match e.value.parse::<i64>() {
            Ok(n) => n,
            Err(_) => {
                return Response::Error {
                    status: "error",
                    message: "not an integer".to_string(),
                }
            }
        },
    };
    let new_val = current + delta;
    map.insert(
        key,
        Entry {
            value: new_val.to_string(),
            expires_at: None,
        },
    );
    Response::OkInt {
        status: "ok",
        value: new_val,
    }
}

// - Sauvegarde l'état complet du store dans un fichier `dump.json` dans le
//   répertoire courant du serveur
// - Format du fichier : libre (un objet JSON `{"key": "value", ...}` suffit)
// - La réponse est `{"status": "ok"}`

async fn cmd_save(store: &Store) -> Response {
    let map = store.lock().await;
    let mut data = HashMap::new();
    for (k, e) in map.iter() {
        data.insert(k.clone(), e.value.clone());
    }
    match serde_json::to_string(&data) {
        Err(_) => Response::Error {
            status: "error",
            message: "serialization failed".to_string(),
        },
        Ok(json) => match std::fs::write("dump.json", json) {
            Ok(_) => Response::Ok { status: "ok" },
            Err(e) => Response::Error {
                status: "error",
                message: e.to_string(),
            },
        },
    }
}
