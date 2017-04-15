extern crate regex;

use std::fmt::Debug;
use std::time::Duration;
use std::collections::HashMap;
use self::regex::Regex;


#[derive(Debug)]
pub enum MouseButton
{
    Left,
    Right,
    Middle,
}

#[derive(Debug)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub fn from_string(string: &str) -> Color {
        let re = Regex::new(r"^#([A-Fa-f0-9]{2})([A-Fa-f0-9]{2})([A-Fa-f0-9]{2})$").unwrap();
        let colors = re.captures(string).unwrap();

        Color(u8::from_str_radix(colors.get(1).unwrap().as_str(), 16).unwrap(),
              u8::from_str_radix(colors.get(2).unwrap().as_str(), 16).unwrap(),
              u8::from_str_radix(colors.get(3).unwrap().as_str(), 16).unwrap())
    }

    pub fn to_string(&self) -> String {
        format!("#{:X}{:X}{:X}", self.0, self.1, self.2)
    }
}

#[derive(Debug)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub info: Color,
    pub warn: Color,
    pub crit: Color,
    pub seperator: Color,
}

pub trait Block
{
    fn id(&self) -> Option<&str> { None }
    fn get_status(&self, theme: &Theme) -> HashMap<&str, String>;
    fn update(&self) -> Option<Duration> { None }
    // NOTE: this function can only be called if id was specified
    fn click(&self, button: MouseButton) {}
}