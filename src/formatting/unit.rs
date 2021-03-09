use std::fmt;

#[derive(Debug, PartialEq, Clone)]
pub enum Unit {
    BitsPerSecond,
    BytesPerSecond,
    Percents,
    Degrees,
    Seconds,
    Watts,
    Hertz,
    Bytes,
    None,
    Other(String), //TODO: do not allow custom units?
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::BitsPerSecond => "Bi/s",
                Self::BytesPerSecond => "B/s",
                Self::Percents => "%",
                Self::Degrees => "°",
                Self::Seconds => "s",
                Self::Watts => "W",
                Self::Hertz => "Hz",
                Self::Bytes => "B",
                Self::None => "",
                Self::Other(unit) => unit.as_str(),
            }
        )
    }
}

impl Unit {
    pub fn from_string(s: &str) -> Self {
        match s {
            "Bi/s" => Self::BitsPerSecond,
            "B/s" => Self::BytesPerSecond,
            "%" => Self::Percents,
            "°" => Self::Degrees,
            "s" => Self::Seconds,
            "W" => Self::Watts,
            "Hz" => Self::Hertz,
            "B" => Self::Bytes,
            "" => Self::None,
            x => Self::Other(x.to_string()),
        }
    }
}
