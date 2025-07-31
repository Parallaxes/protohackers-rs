//! Binary protocol parsing for the Speed Camera System.
//!
//! This module handles parsing of incoming binary messages from clients.
//! The protocol uses big-endian byte order and follows a type-length-value
//! format for strings.
//!
//! # Error Handling
//!
//! Parsing errors are distinct from protocol validation errors:
//! - [`ParseError`] indicates malformed binary data or invalid message types
//! - Protocol validation (client state, business rules) is handled elsewhere
//!
//! # Message Format
//!
//! Each message starts with a single `u8` message type, followed by
//! message-specific fields in big-endian byte order:
//!
//! ```text
//! +--------+------------------+
//! | Type   | Message Data     |
//! | (u8)   | (variable)       |
//! +--------+------------------+
//! ```

use super::{constants::*, messages::*};

/// Errors that can occur when parsing binary protocol data.
#[derive(Debug, PartialEq)]
pub enum ParseError {
    /// Unknown or invalid message type byte
    InvalidMessageType(u8),
    /// Not enough bytes available to complete parsing
    InsufficientData,
    /// String data contains invalid UTF-8 sequences
    InvalidUtf8,
    /// Data format doesn't match expected structure
    InvalidFormat,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::InvalidMessageType(t) => write!(f, "Invalid message type: 0x{:02x}", t),
            ParseError::InsufficientData => write!(f, "Insufficient data for parsing"),
            ParseError::InvalidUtf8 => write!(f, "Invalid UTF-8 in string data"),
            ParseError::InvalidFormat => write!(f, "Invalid data format"),
        }
    }
}

impl std::error::Error for ParseError {}

// Helper functions for reading primitive types

/// Read a big-endian `u16` from the data at the current cursor position.
///
/// Advances the cursor by 2 bytes on success.
fn read_u16_be(data: &[u8], cursor: &mut usize) -> Result<u16, ParseError> {
    let bytes = data
        .get(*cursor..*cursor + 2)
        .ok_or(ParseError::InsufficientData)?;
    *cursor += 2;
    Ok(u16::from_be_bytes(bytes.try_into().unwrap()))
}

/// Read a big-endian `u32` from the data at the current cursor position.
///
/// Advances the cursor by 4 bytes on success.
fn read_u32_be(data: &[u8], cursor: &mut usize) -> Result<u32, ParseError> {
    let bytes = data
        .get(*cursor..*cursor + 4)
        .ok_or(ParseError::InsufficientData)?;
    *cursor += 4;
    Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
}

/// Read a length-prefixed string from the data at the current cursor position.
///
/// Format: `[length: u8][data: u8; length]`
///
/// Advances the cursor by `1 + length` bytes on success.
fn read_string(data: &[u8], cursor: &mut usize) -> Result<String, ParseError> {
    let len = *data.get(*cursor).ok_or(ParseError::InsufficientData)? as usize;
    *cursor += 1;

    let bytes = data
        .get(*cursor..*cursor + len)
        .ok_or(ParseError::InsufficientData)?;
    *cursor += len;

    String::from_utf8(bytes.to_vec()).map_err(|_| ParseError::InvalidUtf8)
}

/// Parse a complete message from binary data.
///
/// # Arguments
///
/// * `data` - Raw message bytes including the message type byte
///
/// # Returns
///
/// * `Ok(Message)` - Successfully parsed message
/// * `Err(ParseError)` - Malformed data or unsupported message type
///
/// # Examples
///
/// ```rust
/// let plate_data = [0x20, 0x04, b'T', b'E', b'S', b'T', 0x00, 0x00, 0x03, 0xe8];
/// let message = parse(&plate_data)?;
/// // Returns Message::Plate(Plate { plate: "TEST", timestamp: 1000 })
/// ```
pub fn parse(data: &[u8]) -> Result<Message, ParseError> {
    if data.is_empty() {
        return Err(ParseError::InsufficientData);
    }

    match data[0] {
        MSG_ERROR => Err(ParseError::InvalidMessageType(MSG_ERROR)), // Clients shouldn't send Error
        MSG_PLATE => parse_plate(&data[1..]),
        MSG_TICKET => Err(ParseError::InvalidMessageType(MSG_TICKET)), // Clients shouldn't send Ticket
        MSG_WANT_HEARTBEAT => parse_wantheartbeat(&data[1..]),
        MSG_IAM_CAMERA => parse_iamcamera(&data[1..]),
        MSG_IAM_DISPATCHER => parse_iamdispatcher(&data[1..]),
        unknown => Err(ParseError::InvalidMessageType(unknown)),
    }
}

/// Parse a Plate message (0x20) from message data.
fn parse_plate(data: &[u8]) -> Result<Message, ParseError> {
    let mut cursor = 0;

    let plate = read_string(data, &mut cursor)?;
    let timestamp = read_u32_be(data, &mut cursor)?;

    Ok(Message::Plate(Plate { plate, timestamp }))
}

/// Parse a WantHeartbeat message (0x40) from message data.
fn parse_wantheartbeat(data: &[u8]) -> Result<Message, ParseError> {
    let mut cursor = 0;
    let interval = read_u32_be(data, &mut cursor)?;

    Ok(Message::WantHeartbeat(WantHeartbeat { interval }))
}

/// Parse an IAmCamera message (0x80) from message data.
fn parse_iamcamera(data: &[u8]) -> Result<Message, ParseError> {
    let mut cursor = 0;

    let road = read_u16_be(data, &mut cursor)?;
    let mile = read_u16_be(data, &mut cursor)?;
    let limit = read_u16_be(data, &mut cursor)?;

    Ok(Message::IAmCamera(IAmCamera { road, mile, limit }))
}

/// Parse an IAmDispatcher message (0x81) from message data.
fn parse_iamdispatcher(data: &[u8]) -> Result<Message, ParseError> {
    let mut roads = Vec::new();
    let mut cursor = 0;

    let numroads = *data.get(cursor).ok_or(ParseError::InsufficientData)?;
    cursor += 1;

    for _ in 0..numroads {
        let road = read_u16_be(data, &mut cursor)?;
        roads.push(road);
    }

    Ok(Message::IAmDispatcher(IAmDispatcher { numroads, roads }))
}

// Note: parse_ticket is implemented but not used since clients don't send tickets
