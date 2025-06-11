use serde::{Serialize, Deserialize};
use serde_json::{Result, Value};

#[derive(Serialize, Deserialize)]
struct Request {
    method: String,
    number: f64,
}

impl Request {
    fn from_json(input: &str) -> Request {
        serde_json::from_str(input).expect("Failed to deserialize JSON")
    }
}