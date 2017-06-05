extern crate serde_json;

use std::io;
use std::option::Option;
use std::string::*;
use std::sync::mpsc::Sender;
use std::thread;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct I3BarEvent {
    pub name: Option<String>,
    pub instance: Option<String>,
    pub x: u64,
    pub y: u64,
    // Button Codes: 1 -> Left, 2 -> Middle, 3 -> Right
    pub button: u64,
}

pub fn process_events(sender: Sender<I3BarEvent>) {
    thread::spawn(move || loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        if input.starts_with(",") {
            let input = input.split_off(1);

            let e: I3BarEvent = serde_json::from_str(&input).unwrap();

            sender.send(e).unwrap();
        }
    });
}
