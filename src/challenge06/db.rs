use std::collections::{BTreeMap, HashSet};
use tokio::net::TcpStream;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::challenge06::utils::{Client, ClientType, IAmCamera, IAmDispatcher};

#[derive(Debug)]
pub enum AssignError {
    InvalidClientType,
}

pub async fn assign_id(
    peer_addr: SocketAddr,
    id_db: Arc<Mutex<BTreeMap<SocketAddr, Client>>>,
    client_type: ClientType,
) -> Result<(), AssignError> {
    let mut db_guard = id_db.lock().await;
    let new_id = generate_next_id(&db_guard);
    let new_client = Client::new(new_id, client_type);

    db_guard.insert(peer_addr, new_client);
    Ok(())
}

fn generate_next_id(db: &BTreeMap<SocketAddr, Client>) -> u32 {
    db.values()
        .map(|client| client.get_id())
        .max()
        .map(|max_id| max_id + 1)
        .unwrap_or(0)
}

