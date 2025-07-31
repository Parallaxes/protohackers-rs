//! Protocol message type constants for the Speed Camera System.
//!
//! These constants define the message type identifiers used in the binary protocol.
//! Each message starts with one of these bytes to indicate its type.

/// Error message sent by server when client violates protocol (Server->Client)
pub const MSG_ERROR: u8 = 0x10;

/// Number plate observation report (Client->Server)
pub const MSG_PLATE: u8 = 0x20;

/// Speed violation ticket (Server->Client)
pub const MSG_TICKET: u8 = 0x21;

/// Heartbeat request (Client->Server)
pub const MSG_WANT_HEARTBEAT: u8 = 0x40;

/// Heartbeat response (Server->Client)  
pub const MSG_HEARTBEAT: u8 = 0x41;

/// Camera identification message (Client->Server)
pub const MSG_IAM_CAMERA: u8 = 0x80;

/// Dispatcher identification message (Client->Server)
pub const MSG_IAM_DISPATCHER: u8 = 0x81;
