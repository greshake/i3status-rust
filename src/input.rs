extern crate serde_json;

use std::sync::mpsc::{Sender};
use std::thread;
use serde_json::{Value, Error};
use block::{Block, MouseButton};
use std::io;
use std::option::Option;
use std::string::*;

use std::io::prelude::*;
use std::fs::File;
use std::path::Path;
use std::fs::OpenOptions;


#[derive(Serialize, Deserialize, Debug)]
pub struct I3barEvent {
    pub name: Option<String>,
    pub instance: Option<String>,
    pub x: u64,
    pub y: u64,
    pub button: u64
}

pub fn process_events(sender: Sender<I3barEvent>) {
    thread::spawn(move || {
        loop {
            let mut input = String::new();
            io::stdin().read_line(&mut input);

            if input.starts_with(",") {
                let input = input.split_off(1);

                let e: I3barEvent = serde_json::from_str(&input).unwrap();

                sender.send(e);
            }
        }
    });
}