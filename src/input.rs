use serde::{de, Deserializer};
use serde_json;
use std::fmt;
use std::io;
use std::option::Option;
use std::string::*;
use std::sync::mpsc::Sender;
use std::thread;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseButton {
    LeftClick,
    RightClick,
    MiddleClick,
    WheelUp,
    WheelDown,
}

#[derive(Deserialize, Debug, Clone)]
pub struct I3BarEvent {
    pub name: Option<String>,
    pub instance: Option<String>,
    pub x: u64,
    pub y: u64,

    #[serde(deserialize_with = "deserialize_mousebutton")]
    pub button: MouseButton,
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

fn deserialize_mousebutton<'de, D>(deserializer: D) -> Result<MouseButton, D::Error>
where
    D: Deserializer<'de>
{
    struct MouseButtonVisitor;

    impl<'de> de::Visitor<'de> for MouseButtonVisitor {
        type Value = MouseButton;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("u64")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error
        {
            Ok(match value {
                   1 => MouseButton::LeftClick,
                   2 => MouseButton::RightClick,
                   3 => MouseButton::MiddleClick,
                   4 => MouseButton::WheelUp,
                   5 => MouseButton::WheelDown,
                   _ => return Err(de::Error::custom("unknown mouse button")),
               })
        }
    }

    deserializer.deserialize_any(MouseButtonVisitor)
}
