use std::fmt;
use std::io;
use std::option::Option;
use std::string::*;
use std::thread;

use crossbeam_channel::Sender;
use futures::{Stream, StreamExt};
use serde::{de, Deserializer};
use serde_derive::Deserialize;
use tokio::io::{stdin, AsyncBufRead, AsyncBufReadExt, BufReader, Lines};
use tokio_stream::wrappers::LinesStream;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
struct I3BarEventInternal {
    pub name: Option<String>,
    pub instance: Option<String>,
    pub x: u64,
    pub y: u64,

    #[serde(deserialize_with = "deserialize_mousebutton")]
    pub button: MouseButton,
}

#[derive(Debug, Clone)]
pub struct I3BarEvent {
    pub id: Option<usize>,
    pub instance: Option<usize>,
    pub button: MouseButton,
}

impl I3BarEvent {
    pub fn matches_id(&self, other: usize) -> bool {
        match self.id {
            Some(id) => id == other,
            _ => false,
        }
    }
}

pub fn input_events() -> impl Stream<Item = I3BarEvent> {
    LinesStream::new(BufReader::new(stdin()).lines())
        .map(|input| input.expect("error while reading input event"))
        .filter_map(|input| async move {
            let input = input
                .trim_start_matches(|c| c != '{')
                .trim_end_matches(|c| c != '}');

            if input.is_empty() {
                return None;
            }

            let e: I3BarEventInternal =
                serde_json::from_str(&input).expect("failed parsing input event");

            Some(I3BarEvent {
                id: e.name.map(|x| x.parse::<usize>().unwrap()),
                instance: e.instance.map(|x| x.parse::<usize>().unwrap()),
                button: e.button,
            })
        })
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
            // TODO: put this behind `--debug` flag
            //eprintln!("{}", value);
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
