pub trait Serialize {
    fn serialize(self) -> Message;
}

#[derive(PartialEq, Debug)]
pub enum Message {
    Error(Error),
    Plate(Plate),
    Ticket(Ticket),
    WantHeartbeat(WantHeartbeat),
    IAmCamera(IAmCamera),
    IAmDispatcher(IAmDispatcher),
}

#[derive(PartialEq, Debug)]
pub struct Client {
    id: u32,
    client_type: ClientType,
}

impl Client {
    pub fn new(id: u32, client_type: ClientType) -> Client {
        Client { id, client_type }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }
}

#[derive(PartialEq, Debug)]
pub enum ClientType {
    Camera,
    Dispatcher,
    Unknown,
}

#[derive(PartialEq, Debug)]
pub struct Error {
    msg: String,
}

#[derive(PartialEq, Debug)]
pub struct Plate {
    plate: String,
    timestamp: u32,
}

#[derive(PartialEq, Debug)]
pub struct Ticket {
    plate: String,
    road: u16,
    mile1: u16,
    timestamp1: u32,
    mile2: u16,
    timestamp2: u32,
    speed: u16,
}

#[derive(PartialEq, Debug)]
pub struct WantHeartbeat {
    interval: u32,
}

#[derive(PartialEq, Debug)]
pub struct Heartbeat {}

#[derive(PartialEq, Debug)]
pub struct IAmCamera {
    road: u16,
    mile: u16,
    limit: u16,
}

#[derive(PartialEq, Debug)]
pub struct IAmDispatcher {
    numroads: u8,
    roads: Vec<u16>,
}

pub fn parse(data: &[u8]) -> Option<Message> {
    if data.len() < 1 {
        return None;
    }

    match data[0] {
        0x10 => Some(Message::Error(Error {
            msg: "illegal msg".to_string(),
        })),
        0x20 => parse_plate(&data[1..]),
        0x21 => parse_ticket(&data[1..]),
        0x40 => parse_wantheartbeat(&data[1..]),
        0x80 => parse_iamcamera(&data[1..]),
        0x81 => parse_iamdispatcher(&data[1..]),
        _ => None,
    }
}

fn parse_plate(data: &[u8]) -> Option<Message> {
    let mut cursor = 0;

    let plate_len = *data.get(cursor)? as usize;
    cursor += 1;

    let plate_bytes = data.get(cursor..cursor + plate_len)?;
    let plate = std::str::from_utf8(plate_bytes).ok()?.to_string();
    cursor += plate_len;

    let timestamp = u32::from_be_bytes(data.get(cursor..cursor + 4)?.try_into().ok()?);

    Some(Message::Plate(Plate { plate, timestamp }))
}

fn parse_ticket(data: &[u8]) -> Option<Message> {
    let mut cursor = 0;

    let plate_len = *data.get(cursor)? as usize;
    cursor += 1;

    let plate_bytes = data.get(cursor..cursor + plate_len)?;
    let plate = std::str::from_utf8(plate_bytes).ok()?.to_string();
    cursor += plate_len;

    let road = u16::from_be_bytes(data.get(cursor..cursor + 2)?.try_into().ok()?);
    cursor += 2;

    let mile1 = u16::from_be_bytes(data.get(cursor..cursor + 2)?.try_into().ok()?);
    cursor += 2;

    let timestamp1 = u32::from_be_bytes(data.get(cursor..cursor + 4)?.try_into().ok()?);
    cursor += 4;

    let mile2 = u16::from_be_bytes(data.get(cursor..cursor + 2)?.try_into().ok()?);
    cursor += 2;

    let timestamp2 = u32::from_be_bytes(data.get(cursor..cursor + 4)?.try_into().ok()?);
    cursor += 4;

    let speed = u16::from_be_bytes(data.get(cursor..cursor + 2)?.try_into().ok()?);

    Some(Message::Ticket(Ticket {
        plate,
        road,
        mile1,
        timestamp1,
        mile2,
        timestamp2,
        speed,
    }))
}

fn parse_wantheartbeat(data: &[u8]) -> Option<Message> {
    let interval = u32::from_be_bytes(data.get(0..)?.try_into().ok()?);

    Some(Message::WantHeartbeat(WantHeartbeat { interval }))
}

fn parse_iamcamera(data: &[u8]) -> Option<Message> {
    let mut cursor = 0;

    let road = u16::from_be_bytes(data.get(cursor..cursor + 2)?.try_into().ok()?);
    cursor += 2;

    let mile = u16::from_be_bytes(data.get(cursor..cursor + 2)?.try_into().ok()?);
    cursor += 2;

    let limit = u16::from_be_bytes(data.get(cursor..cursor + 2)?.try_into().ok()?);

    Some(Message::IAmCamera(IAmCamera { road, mile, limit }))
}

fn parse_iamdispatcher(data: &[u8]) -> Option<Message> {
    let mut roads = Vec::new();
    let mut cursor = 1;

    let numroads = *data.get(0)?;
    for _ in 0..numroads {
        let road = u16::from_be_bytes(data.get(cursor..cursor + 2)?.try_into().ok()?);
        roads.push(road);
        cursor += 2;
    }

    Some(Message::IAmDispatcher(IAmDispatcher { numroads, roads }))
}

// mod tests {
//     use super::*;

//     #[test]
//     fn test_parse_plate1() {
//         let data = [0x04, 0x55, 0x4e, 0x31, 0x58, 0x00, 0x00, 0x03, 0xe8];
//         let expected = Message::Plate(Plate {
//             plate: "UN1X".to_string(),
//             timestamp: 1000,
//         });
//         if let Some(result) = parse_plate(&data) {
//             assert_eq!(result, expected);
//         }
//     }

//     #[test]
//     fn test_parse_plate2() {
//         let data = [
//             0x07, 0x52, 0x45, 0x30, 0x35, 0x42, 0x4b, 0x47, 0x00, 0x01, 0xe2, 0x40,
//         ];
//         let expected = Message::Plate(Plate {
//             plate: "RE05BKG".to_string(),
//             timestamp: 123456,
//         });
//         if let Some(result) = parse_plate(&data) {
//             assert_eq!(result, expected);
//         } else {
//             panic!("Failed!")
//         }
//     }

//     #[test]
//     fn test_parse_ticket1() {
//         let data = [
//             0x04, 0x55, 0x4e, 0x31, 0x58, 0x00, 0x42, 0x00, 0x64, 0x00, 0x01, 0xe2, 0x40, 0x00,
//             0x6e, 0x00, 0x01, 0xe3, 0xa8, 0x27, 0x10,
//         ];
//         let expected = Message::Ticket(Ticket {
//             plate: "UN1X".to_string(),
//             road: 66,
//             mile1: 100,
//             timestamp1: 123456,
//             mile2: 110,
//             timestamp2: 123816,
//             speed: 10000,
//         });
//         if let Some(result) = parse_ticket(&data) {
//             assert_eq!(result, expected);
//         } else {
//             panic!("Failed!")
//         }
//     }
// }
