const MSG_ERROR: u8 = 0x10;
const MSG_PLATE: u8 = 0x20;
const MSG_TICKET: u8 = 0x21;
const MSG_WANT_HEARTBEAT: u8 = 0x40;
const MSG_IAM_CAMERA: u8 = 0x80;
const MSG_IAM_DISPATCHER: u8 = 0x81;

#[derive(Debug)]
pub enum ParseError {
    InvalidMessageType(u8),
    InsufficientData,
    InvalidUtf8,
    InvalidFormat,
}

pub fn parse(data: &[u8]) -> Result<Message, ParseError> {
    if data.is_empty() {
        return Err(ParseError::InsufficientData);
    }

    match data[0] {
        MSG_ERROR => parse_plate(&data[1..]),
    }
}

fn read_u16_be(data: &[u8], cursor: &mut usize) -> Result<u16, ParseError> {
    let bytes = data.get(*cursor..*cursor + 2)
        .ok_or(ParseError::InsufficientData)?;
    *cursor += 2;
    Ok(u16::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_u32_be(data: &[u8], cursor: &mut usize) -> Result<u32, ParseError> {
    let bytes = data.get(*cursor..*cursor + 4)
        .ok_or(ParseError::InsufficientData)?;
    *cursor += 4;
    Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_string(data: &[u8], cursor: &mut usize) -> Result<String, ParseError> {
    let len = *data.get(*cursor).ok_or(ParseError::InsufficientData)? as usize;
    *cursor += 1;

    let bytes = data.get(*cursor..*cursor + len)
        .ok_or(ParseError::InsufficientData)?;
    *cursor += len;

    String::from_utf8(bytes.to_vec())
        .map_err(|_| ParseError::InvalidUtf8)
}