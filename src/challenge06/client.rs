//! Client management for the SCS.
//! 
//! Tracks client connection state and identification information.

/// Represents a fully identified client with assigned ID.
#[derive(PartialEq, Debug, Clone)]
pub struct Client {
    id: u32,
    client_type: ClientType,
}

/// Types of clients that can connect to the server
#[derive(PartialEq, Debug, Clone)]
pub enum ClientType {
    /// Camera client that reports plate observations
    Camera { road: u16, mile: u16, limit: u16},
    /// Dispatcher client that receives tickets for specific roads
    Dispatcher { roads: Vec<u16> },
}

/// Tracks the current state of a client connection
/// 
/// Used before the client has fully identified itself and been assigned an ID.
#[derive(Debug)]
pub struct ClientState {
    /// Client type and details (None until they identify themselves)
    pub client_type: Option<ClientType>,
    /// Whether client has requested heartbeats
    pub has_heartbeat: bool,
    /// Assigned client ID (None until identification complete)
    pub client_id: Option<u32>,
}

impl ClientState {
    /// Create a new unidentified client state.
    pub fn new() -> Self {
        Self {
            client_type: None,
            has_heartbeat: false,
            client_id: None,
        }
    }

    /// Check if the client has identified itself.
    pub fn is_identified(&self) -> bool {
        self.client_type.is_some()
    }

    pub fn set_client_type(&mut self, client_type: ClientType) {
        self.client_type = Some(client_type);
    }

    /// Convert to a fully identified Client with assigned ID.
    pub fn to_client(&self, id: u32) -> Option<Client> {
        self.client_type.as_ref().map(|ct| Client {
            id,
            client_type: ct.clone(),
        })
    }
}

impl Client {
    /// Create a new identified client.
    pub fn new(id: u32, client_type: ClientType) -> Self {
        Client { id, client_type }
    }
    
    /// Get the client's assigned ID.
    pub fn get_id(&self) -> u32 {
        self.id
    }

    /// Get the client's type and details
    pub fn get_type(&self) -> &ClientType {
        &self.client_type
    }
}