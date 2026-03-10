use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

mod command;
use command::{handle_command};
type Store = Arc<Mutex<HashMap<String, String>>>;
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
    // Bind un TcpListener sur 127.0.0.1:7878 : 
    let listener = TcpListener::bind("127.0.0.1:7878").await
        .expect("Impossible de se connecter a 127.0.0.1 avec le port 7878 vérifie si le port est en use") ;
    loop {
        let (socket, addr) = listener.accept().await.unwrap();
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
    let (read_half, mut write_half) = socket.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,   // client déconnecté
            Ok(_) => {}
            Err(_) => break,  // erreur réseau
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