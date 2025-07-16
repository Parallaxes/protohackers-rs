use tokio::sync::Mutex;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::challenge06::protocol::IAmCamera;

use super::{
    client::{Client, ClientType},
    protocol::Plate,
};

#[derive(Debug)]
pub enum AssignError {
    InvalidClientType,
}

#[derive(Debug, Clone)]
pub struct PlateObservation {
    pub plate: Plate,
    pub camera: IAmCamera,
    pub camera_id: u32,
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
    
}