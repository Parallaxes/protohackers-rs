use tokio::sync::Mutex;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use super::client::*;


#[derive(Debug)]
pub enum AssignError {
    InvalidClientType,
}

pub struct Database<T, U> {
    db: Arc<Mutex<BTreeMap<T, U>>>,
}

impl<T, U> Database<T, U> {
    pub fn new() -> Self {
        Self {
            db: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub async fn insert(&mut self, key: T, value: U) -> Option<U>
    where
        T: Ord, 
    {
        let mut db_guard = self.db.lock().await;
        db_guard.insert(key, value)
    }

    pub async fn remove(&self, key: &T) -> Option<U>
    where
        T: Ord,
    {
        let mut db_guard = self.db.lock().await;
        db_guard.remove(key)
    }

    pub async fn get(&self, key: &T) -> Option<U>
    where
        T: Ord,
        U: Clone,
    {
        let db_guard = self.db.lock().await;
        db_guard.get(key).cloned()
    }

    pub fn clone_handle(&self) -> Self {
        Self {
            db: self.db.clone(),
        }
    }
}

impl Database<SocketAddr, Client> {
    pub async fn assign_id(
        &self,
        peer_addr: SocketAddr,
        client_type: ClientType,
    ) -> Result<(), AssignError> {
        let mut db_guard = self.db.lock().await;
        let new_id = Self::generate_next_id(&db_guard);
        let new_client = Client::new(new_id, client_type);

        db_guard.insert(peer_addr, new_client);
        Ok(())
    }

    pub fn generate_next_id(db: &BTreeMap<SocketAddr, Client>) -> u32 {
        db.values()
            .map(|client| client.get_id())
            .max()
            .map(|max_id| max_id + 1)
            .unwrap_or(0)
    }
}