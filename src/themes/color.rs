use crate::errors::*;
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use smart_default::SmartDefault;
use std::fmt;
use std::ops::Add;
use std::str::FromStr;

/// An RGBA color (red, green, blue, alpha).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
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
    /// `a`: alpha component (0 to 255).
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Create a new RGBA color from the `hex` value.
    ///
    /// ```let cyan = Rgba::from_hex(0xffffff);```
    pub fn from_hex(hex: u32) -> Self {
        let [r, g, b, a] = hex.to_be_bytes();
        Self { r, g, b, a }
    }
}

impl Add for Rgba {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Rgba::new(
            self.r.saturating_add(rhs.r),
            self.g.saturating_add(rhs.g),
            self.b.saturating_add(rhs.b),
            self.a.saturating_add(rhs.a),
        )
    }
}

/// An HSVA color (hue, saturation, value, alpha).
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
    /// `a`: alpha component (0 to 255).
    pub fn new(h: f64, s: f64, v: f64, a: u8) -> Self {
        Self { h, s, v, a }
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
    fn from(rgba: Rgba) -> Self {
        let r = rgba.r as f64 / 255.0;
        let g = rgba.g as f64 / 255.0;
        let b = rgba.b as f64 / 255.0;

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
}

impl From<Hsva> for Rgba {
    fn from(hsva: Hsva) -> Self {
        let range = (hsva.h / 60.0) as u8;
        let c = hsva.v * hsva.s;
        let x = c * (1.0 - (((hsva.h / 60.0) % 2.0) - 1.0).abs());
        let m = hsva.v - c;

        let cm_scaled = ((c + m) * 255.0) as u8;
        let xm_scaled = ((x + m) * 255.0) as u8;
        let m_scaled = (m * 255.0) as u8;

        match range {
            0 => Self::new(cm_scaled, xm_scaled, m_scaled, hsva.a),
            1 => Self::new(xm_scaled, cm_scaled, m_scaled, hsva.a),
            2 => Self::new(m_scaled, cm_scaled, xm_scaled, hsva.a),
            3 => Self::new(m_scaled, xm_scaled, cm_scaled, hsva.a),
            4 => Self::new(xm_scaled, m_scaled, cm_scaled, hsva.a),
            _ => Self::new(cm_scaled, m_scaled, xm_scaled, hsva.a),
        }
    }
}

impl Add for Hsva {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Hsva::new(
            (self.h + rhs.h) % 360.,
            (self.s + rhs.s).clamp(0., 1.),
            (self.v + rhs.v).clamp(0., 1.),
            self.a.saturating_add(rhs.a),
        )
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
        match (self, rhs) {
            // Do nothing
            (x, Self::None | Self::Auto) | (Self::None | Self::Auto, x) => x,
            // Hsva + Hsva => Hsva
            (Color::Hsva(hsva1), Color::Hsva(hsva2)) => Color::Hsva(hsva1 + hsva2),
            // Rgba + Rgba => Rgba
            (Color::Rgba(rgba1), Color::Rgba(rgba2)) => Color::Rgba(rgba1 + rgba2),
            // Hsva + Rgba => Hsva
            // Rgba + Hsva => Hsva
            (Color::Hsva(hsva), Color::Rgba(rgba)) | (Color::Rgba(rgba), Color::Hsva(hsva)) => {
                Color::Hsva(hsva + rgba.into())
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
            let err_msg = || format!("'{color}' is not a valid HSVA color");
            let color = color.split_at(4).1;
            let mut components = color.split(':').map(|x| x.parse::<f64>().or_error(err_msg));
            let h = components.next().or_error(err_msg)??;
            let s = components.next().or_error(err_msg)??;
            let v = components.next().or_error(err_msg)??;
            let a = components.next().unwrap_or(Ok(100.))?;
            Color::Hsva(Hsva::new(h, s / 100., v / 100., (a / 100. * 255.) as u8))
        } else {
            let err_msg = || format!("'{color}' is not a valid RGBA color");
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
        let format_rgba =
            |rgba: Rgba| format!("#{:02X}{:02X}{:02X}{:02X}", rgba.r, rgba.g, rgba.b, rgba.a);
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
