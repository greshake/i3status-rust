use crate::errors::*;
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use smart_default::SmartDefault;
use std::fmt;
use std::ops::Add;
use std::str::FromStr;

/// An RGBA color (red, green, blue, alpha).
#[derive(Copy, Clone, Debug, Default)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: u8,
}

impl Rgba {
    /// Create a new RGBA color.
    ///
    /// `r`: red component (0 to 255).
    ///
    /// `g`: green component (0 to 255).
    ///
    /// `b`: blue component (0 to 255).
    ///
    /// `a`: alpha component (0 to 100).
    #[inline]
    pub fn new(r: f64, g: f64, b: f64, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Create a new RGBA color from the `hex` value.
    ///
    /// ```let cyan = Rgba::from_hex(0xffffff);```
    pub fn from_hex(hex: u32) -> Self {
        Self {
            r: (((hex >> 24) & 0xff) as f64),
            g: (((hex >> 16) & 0xff) as f64),
            b: (((hex >> 8) & 0xff) as f64),
            a: ((hex & 0xff) as u8),
        }
    }
}

impl PartialEq for Rgba {
    fn eq(&self, other: &Self) -> bool {
        approx(self.r, other.r)
            && approx(self.g, other.g)
            && approx(self.b, other.b)
            && self.a == other.a
    }
}

/// An HSV color (hue, saturation, value).
#[derive(Copy, Clone, Debug, Default)]
pub struct Hsva {
    pub h: f64,
    pub s: f64,
    pub v: f64,
    pub a: u8,
}

impl Hsva {
    /// Create a new HSVA color.
    ///
    /// `h`: hue component (0 to 360)
    ///
    /// `s`: saturation component (0 to 1)
    ///
    /// `v`: value component (0 to 1)
    ///
    /// `a`: alpha component (0 to 100).
    #[inline]
    pub fn new(h: f64, s: f64, v: f64, a: u8) -> Self {
        Self { h, s, v, a }
    }

    fn from_rgba(rgba: &Rgba) -> Self {
        let r = rgba.r / 255.0;
        let g = rgba.g / 255.0;
        let b = rgba.b / 255.0;

        let min = r.min(g.min(b));
        let max = r.max(g.max(b));
        let delta = max - min;

        let v = max;
        let s = match max > 1e-3 {
            true => delta / max,
            false => 0.0,
        };
        let h = match delta == 0.0 {
            true => 0.0,
            false => {
                if r == max {
                    (g - b) / delta
                } else if g == max {
                    2.0 + (b - r) / delta
                } else {
                    4.0 + (r - g) / delta
                }
            }
        };
        let h2 = ((h * 60.0) + 360.0) % 360.0;

        Self::new(h2, s, v, rgba.a)
    }

    fn to_rgba(self) -> Rgba {
        let range = (self.h / 60.0) as u8;
        let c = self.v * self.s;
        let x = c * (1.0 - (((self.h / 60.0) % 2.0) - 1.0).abs());
        let m = self.v - c;

        match range {
            0 => Rgba::new((c + m) * 255.0, (x + m) * 255.0, m * 255.0, self.a),
            1 => Rgba::new((x + m) * 255.0, (c + m) * 255.0, m * 255.0, self.a),
            2 => Rgba::new(m * 255.0, (c + m) * 255.0, (x + m) * 255.0, self.a),
            3 => Rgba::new(m * 255.0, (x + m) * 255.0, (c + m) * 255.0, self.a),
            4 => Rgba::new((x + m) * 255.0, m * 255.0, (c + m) * 255.0, self.a),
            _ => Rgba::new((c + m) * 255.0, m * 255.0, (x + m) * 255.0, self.a),
        }
    }
}

impl PartialEq for Hsva {
    fn eq(&self, other: &Self) -> bool {
        approx(self.h, other.h)
            && approx(self.s, other.s)
            && approx(self.v, other.v)
            && self.a == other.a
    }
}

impl From<Rgba> for Hsva {
    #[inline]
    fn from(color: Rgba) -> Self {
        Self::from_rgba(&color)
    }
}

impl From<Hsva> for Rgba {
    #[inline]
    fn from(color: Hsva) -> Self {
        color.to_rgba()
    }
}

pub fn approx(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    let eps = 1e-2;
    let abs_a = a.abs();
    let abs_b = b.abs();
    let diff = (abs_a - abs_b).abs();
    if a == 0.0 || b == 0.0 || abs_a + abs_b < std::f64::EPSILON {
        diff < eps * std::f64::EPSILON
    } else {
        diff / (abs_a + abs_b).min(std::f64::MAX) < eps
    }
}

#[derive(Debug, Clone, Copy, PartialEq, SmartDefault)]
pub enum Color {
    #[default]
    None,
    Auto,
    Rgba(Rgba),
    Hsva(Hsva),
}

impl Color {
    pub fn skip_ser(&self) -> bool {
        matches!(self, Self::None | Self::Auto)
    }
}

impl Add for Color {
    type Output = Color;
    fn add(self, rhs: Self) -> Self::Output {
        let add_hsva = |hsva1: Hsva, hsva2: Hsva| {
            Hsva::new(
                (hsva1.h + hsva2.h) % 360.,
                (hsva1.s + hsva2.s).clamp(0., 1.),
                (hsva1.v + hsva2.v).clamp(0., 1.),
                hsva1.a.saturating_add(hsva2.a),
            )
        };

        match (self, rhs) {
            // Do nothing
            (x, Self::None | Self::Auto) | (Self::None | Self::Auto, x) => x,
            // Hsva + Hsva => Hsva
            (Color::Hsva(hsva1), Color::Hsva(hsva2)) => Color::Hsva(add_hsva(hsva1, hsva2)),
            // Rgba + Rgba => Rgba
            (Color::Rgba(rgba1), Color::Rgba(rgba2)) => Color::Rgba(Rgba::new(
                (rgba1.r + rgba2.r).clamp(0., 255.),
                (rgba1.g + rgba2.g).clamp(0., 255.),
                (rgba1.b + rgba2.b).clamp(0., 255.),
                rgba1.a.saturating_add(rgba2.a),
            )),
            // Hsva + Rgba => Hsva
            // Rgba + Hsva => Hsva
            (Color::Hsva(hsva), Color::Rgba(rgba)) | (Color::Rgba(rgba), Color::Hsva(hsva)) => {
                Color::Hsva(add_hsva(hsva, rgba.into()))
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
            Color::Hsva(Hsva::new(h, s / 100., v / 100., (a / 100. * 255.) as u8))
        } else {
            let err_msg = || format!("'{}' is not a vaild RGBA color", color);
            let rgb = color.get(1..7).or_error(err_msg)?;
            let a = color.get(7..9).unwrap_or("FF");
            Color::Rgba(Rgba::from_hex(
                (u32::from_str_radix(rgb, 16).or_error(err_msg)? << 8)
                    + u32::from_str_radix(a, 16).or_error(err_msg)?,
            ))
        })
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let format_rgba = |rgba: Rgba| {
            format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                rgba.r as u8, rgba.g as u8, rgba.b as u8, rgba.a
            )
        };
        match *self {
            Self::None | Self::Auto => serializer.serialize_none(),
            Self::Rgba(rgba) => serializer.serialize_str(&format_rgba(rgba)),
            Self::Hsva(hsva) => serializer.serialize_str(&format_rgba(hsva.into())),
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
