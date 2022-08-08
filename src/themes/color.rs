use crate::errors::*;
use color_space::{Hsv, Rgb};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use smart_default::SmartDefault;
use std::fmt;
use std::ops::Add;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, SmartDefault)]
pub enum Color {
    #[default]
    None,
    Auto,
    Rgba(Rgb, u8),
    Hsva(Hsv, u8),
}

impl Color {
    pub fn skip_ser(&self) -> bool {
        matches!(self, Self::None | Self::Auto)
    }
}

impl Add for Color {
    type Output = Color;
    fn add(self, rhs: Self) -> Self::Output {
        let add_hsv = |a: Hsv, b: Hsv| {
            Hsv::new(
                (a.h + b.h) % 360.,
                (a.s + b.s).clamp(0., 1.),
                (a.v + b.v).clamp(0., 1.),
            )
        };

        match (self, rhs) {
            // Do nothing
            (x, Self::None | Self::Auto) | (Self::None | Self::Auto, x) => x,
            // Hsv + Hsv => Hsv
            (Color::Hsva(hsv1, a1), Color::Hsva(hsv2, a2)) => {
                Color::Hsva(add_hsv(hsv1, hsv2), a1.saturating_add(a2))
            }
            // Rgb + Rgb => Rgb
            (Color::Rgba(rgb1, a1), Color::Rgba(rgb2, a2)) => Color::Rgba(
                Rgb::new(
                    (rgb1.r + rgb2.r).clamp(0., 255.),
                    (rgb1.g + rgb2.g).clamp(0., 255.),
                    (rgb1.b + rgb2.b).clamp(0., 255.),
                ),
                a1.saturating_add(a2),
            ),
            // Hsv + Rgb => Hsv
            // Rgb + Hsv => Hsv
            (Color::Hsva(hsv, a1), Color::Rgba(rgb, a2))
            | (Color::Rgba(rgb, a1), Color::Hsva(hsv, a2)) => {
                Color::Hsva(add_hsv(hsv, rgb.into()), a1.saturating_add(a2))
            }
        }
    }
}

impl FromStr for Color {
    type Err = Error;

    fn from_str(color: &str) -> Result<Self, Self::Err> {
        Ok(if color == "none" || color.is_empty() {
            Color::None
        } else if color == "auto" {
            Color::Auto
        } else if color.starts_with("hsv:") {
            let err_msg = || format!("'{}' is not a vaild HSVA color", color);
            let color = color.split_at(4).1;
            let mut components = color.split(':').map(|x| x.parse::<f64>().or_error(err_msg));
            let h = components.next().or_error(err_msg)??;
            let s = components.next().or_error(err_msg)??;
            let v = components.next().or_error(err_msg)??;
            let a = components.next().unwrap_or(Ok(100.))?;
            Color::Hsva(Hsv::new(h, s / 100., v / 100.), (a / 100. * 255.) as u8)
        } else {
            let err_msg = || format!("'{}' is not a vaild RGBA color", color);
            let rgb = color.get(1..7).or_error(err_msg)?;
            let a = color.get(7..9).unwrap_or("FF");
            Color::Rgba(
                Rgb::from_hex(u32::from_str_radix(rgb, 16).or_error(err_msg)?),
                u8::from_str_radix(a, 16).or_error(err_msg)?,
            )
        })
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let format_rgb = |rgb: Rgb, a: u8| {
            format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                rgb.r as u8, rgb.g as u8, rgb.b as u8, a
            )
        };
        match *self {
            Self::None | Self::Auto => serializer.serialize_none(),
            Self::Rgba(rgb, a) => serializer.serialize_str(&format_rgb(rgb, a)),
            Self::Hsva(hsv, a) => serializer.serialize_str(&format_rgb(hsv.into(), a)),
        }
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ColorVisitor;

        impl<'de> Visitor<'de> for ColorVisitor {
            type Value = Color;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("color")
            }

            fn visit_str<E>(self, s: &str) -> Result<Color, E>
            where
                E: de::Error,
            {
                s.parse().serde_error()
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}
