use crate::challenge06::protocol::IAmCamera;

use super::protocol::Plate;

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
