use std::collections::{BTreeMap, HashSet};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::challenge06::utils::{Client, ClientType, IAmCamera, IAmDispatcher};

// TODO: Write tests + refactor
pub async fn assign_id<T>(
    stream: TcpStream,
    id_db: Arc<Mutex<BTreeMap<SocketAddr, Client>>>,
    client: T,
) -> Option<()> {
    let mut db_guard = id_db.lock().await;
    let new_id = db_guard
        .values()
        .map(|client| client.get_id())
        .max()
        .map(|max_id| max_id + 1)
        .unwrap_or(0);
    let new_client = match client {
        IAmCamera => Client::new(new_id, ClientType::Camera),
        IAmDispatcher => Client::new(new_id, ClientType::Dispatcher),
        _ => return None,
    };
    //let new_id = db_guard.values().max().map(|i| i + 1).ok_or("Failed to generate ID").unwrap();
    if let Some(peer_addr) = stream.peer_addr().ok() {
        db_guard.insert(peer_addr, new_client);
        return Some(());
    }

    None
}
