use serde::{de, Deserializer};
use serde_json;
use std::fmt;
use std::io;
use std::option::Option;
use std::string::*;
use chan::Sender;
use std::thread;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    Forward, // On my mouse, these map to forward and back
    Back,
    Unknown,
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

impl I3BarEvent {
    pub fn matches_name(&self, other: &str) -> bool {
        match self.name {
            Some(ref name) => name.as_str() == other,
            _ => false,
        }
    }
}

pub fn process_events(sender: Sender<I3BarEvent>) {
    thread::spawn(move || loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        // Take only the valid JSON object betweem curly braces (cut off leading bracket, commas and whitespace)
        let slice = input.trim_left_matches(|c| c != '{');
        let slice = slice.trim_right_matches(|c| c != '}');

        if !slice.is_empty() {
            let e: I3BarEvent = serde_json::from_str(slice).unwrap();
            sender.send(e);
        }
    });
}

fn deserialize_mousebutton<'de, D>(deserializer: D) -> Result<MouseButton, D::Error>
where
    D: Deserializer<'de>,
{
    struct MouseButtonVisitor;

    impl<'de> de::Visitor<'de> for MouseButtonVisitor {
        type Value = MouseButton;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("u64")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            eprintln!("{}", value);
            Ok(match value {
                1 => MouseButton::Left,
                2 => MouseButton::Middle,
                3 => MouseButton::Right,
                4 => MouseButton::WheelUp,
                5 => MouseButton::WheelDown,
                9 => MouseButton::Forward,
                8 => MouseButton::Back,
                _ => MouseButton::Unknown,
            })
        }
    }

    deserializer.deserialize_any(MouseButtonVisitor)
}
