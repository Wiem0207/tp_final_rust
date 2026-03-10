use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

mod command;
use command::{handle_command, Entry};

// état partagé entre tous les clients comme suggéré dans le sujet :
// Arc<Mutex<HashMap<String, Entry>>> pour gérer les connexions concurrentes
// j'ai étendu String en Entry pour supporter EXPIRE (palier 2)

type Store = Arc<Mutex<HashMap<String, Entry>>>;
#[tokio::main]
async fn main() {
    // Initialiser tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    // 1. Créer le store partagé (Arc<Mutex<HashMap<String, ...>>>) :
    let store: Store = Arc::new(Mutex::new(HashMap::new()));
    // background check to delete expired keys :
    let store_cleaner = Arc::clone(&store);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            let now = Instant::now();
            let mut map = store_cleaner.lock().await;
            map.retain(|_, e| match e.expires_at {
                None => true,
                Some(t) => t > now,
            });
        }
    });
    // Bind un TcpListener sur 127.0.0.1:7878 :
    let listener = TcpListener::bind("127.0.0.1:7878").await.expect(
        "Impossible de se connecter a 127.0.0.1 avec le port 7878 vérifie si le port est en use",
    );
    // 3. accept loop : chaque client est traité dans sa propre tâche Tokio
    loop {
        let (socket, ..) = listener.accept().await.unwrap();
        // cloning the shared store for the next thread
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            handle_client(socket, store).await;
        });
    }

    // TODO: Implémenter le serveur MiniRedis sur 127.0.0.1:7878
    //
    // Étapes suggérées :
    // 1. Créer le store partagé (Arc<Mutex<HashMap<String, ...>>>)
    // 2. Bind un TcpListener sur 127.0.0.1:7878
    // 3. Accept loop : pour chaque connexion, spawn une tâche
    // 4. Dans chaque tâche : lire les requêtes JSON ligne par ligne,
    //    traiter la commande, envoyer la réponse JSON + '\n'
}
async fn handle_client(socket: tokio::net::TcpStream, store: Store) {
    // lecture ligne par ligne avec BufReader
    let (read_half, mut write_half) = socket.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = handle_command(trimmed, &store).await;
        let mut json = serde_json::to_string(&response).unwrap();
        json.push('\n');
        if write_half.write_all(json.as_bytes()).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use command::handle_command;

    fn make_store() -> Store {
        Arc::new(Mutex::new(HashMap::new()))
    }
    #[tokio::test]
    async fn test_ping() {
        let store = make_store();
        let r = handle_command(r#"{"cmd":"PING"}"#, &store).await;
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, r#"{"status":"ok"}"#);
    }
    #[tokio::test]
    async fn test_set() {
        let store = make_store();
        let r = handle_command(
            r#"{"cmd":"SET","key":"ma_cle","value":"ma_valeur"}"#,
            &store,
        )
        .await;
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, r#"{"status":"ok"}"#);
    }
    #[tokio::test]
    async fn test_get() {
        let store = make_store();
        let r = handle_command(r#"{"cmd":"GET","key":"ma_cle"}"#, &store).await;
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, r#"{"status":"ok","value":null}"#);
    }
    #[tokio::test]
    async fn test_invalid_json() {
        let store = make_store();
        let r = handle_command("not json", &store).await;
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"error\""));
    }
    #[tokio::test]
    async fn test_incr() {
        let store = make_store();
        let r = handle_command(r#"{"cmd":"INCR","key":"n"}"#, &store).await;
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"value\":1"));
    }
}
