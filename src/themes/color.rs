use crate::errors::*;
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use smart_default::SmartDefault;
use std::fmt;
use std::ops::Add;
use std::str::FromStr;

pub trait FromRgb {
    /// Convert from an `Rgb` color.
    fn from_rgb(rgb: &Rgb) -> Self;
}

pub trait ToRgb {
    /// Convert into an `Rgb` color.
    fn to_rgb(&self) -> Rgb;
}

pub trait FromColor<T: ToRgb> {
    /// Convert from another color space `T`.
    fn from_color(color: &T) -> Self;
}
/// An RGB color (red, green, blue).
#[derive(Copy, Clone, Debug, Default)]
pub struct Rgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl Rgb {
    /// Create a new RGB color.
    ///
    /// `r`: red component (0 to 255).
    ///
    /// `g`: green component (0 to 255).
    ///
    /// `b`: blue component (0 to 255).
    #[inline]
    pub fn new(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b }
    }

    /// Create a new RGB color from the `hex` value.
    ///
    /// ```let cyan = Rgb::from_hex(0x00ffff);```
    pub fn from_hex(hex: u32) -> Self {
        Self {
            r: (((hex >> 16) & 0xff) as f64),
            g: (((hex >> 8) & 0xff) as f64),
            b: ((hex & 0xff) as f64),
        }
    }
}

impl PartialEq for Rgb {
    fn eq(&self, other: &Self) -> bool {
        approx(self.r, other.r) && approx(self.g, other.g) && approx(self.b, other.b)
    }
}

impl FromRgb for Rgb {
    fn from_rgb(rgb: &Rgb) -> Self {
        *rgb
    }
}

impl ToRgb for Rgb {
    fn to_rgb(&self) -> Rgb {
        *self
    }
}

/// An HSV color (hue, saturation, value).
#[derive(Copy, Clone, Debug, Default)]
pub struct Hsv {
    pub h: f64,
    pub s: f64,
    pub v: f64,
}

impl Hsv {
    /// Create a new HSV color.
    ///
    /// `h`: hue component (0 to 360)
    ///
    /// `s`: saturation component (0 to 1)
    ///
    /// `v`: value component (0 to 1)
    #[inline]
    pub fn new(h: f64, s: f64, v: f64) -> Self {
        Self { h, s, v }
    }
}

impl PartialEq for Hsv {
    fn eq(&self, other: &Self) -> bool {
        approx(self.h, other.h) && approx(self.s, other.s) && approx(self.v, other.v)
    }
}

impl FromRgb for Hsv {
    fn from_rgb(rgb: &Rgb) -> Self {
        let r = rgb.r / 255.0;
        let g = rgb.g / 255.0;
        let b = rgb.b / 255.0;

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

        Self::new(h2, s, v)
    }
}

impl ToRgb for Hsv {
    fn to_rgb(&self) -> Rgb {
        let range = (self.h / 60.0) as u8;
        let c = self.v * self.s;
        let x = c * (1.0 - (((self.h / 60.0) % 2.0) - 1.0).abs());
        let m = self.v - c;

        match range {
            0 => Rgb::new((c + m) * 255.0, (x + m) * 255.0, m * 255.0),
            1 => Rgb::new((x + m) * 255.0, (c + m) * 255.0, m * 255.0),
            2 => Rgb::new(m * 255.0, (c + m) * 255.0, (x + m) * 255.0),
            3 => Rgb::new(m * 255.0, (x + m) * 255.0, (c + m) * 255.0),
            4 => Rgb::new((x + m) * 255.0, m * 255.0, (c + m) * 255.0),
            _ => Rgb::new((c + m) * 255.0, m * 255.0, (x + m) * 255.0),
        }
    }
}

impl FromColor<Rgb> for Rgb {
    #[inline]
    fn from_color(color: &Self) -> Self {
        *color
    }
}

impl FromColor<Rgb> for Hsv {
    #[inline]
    fn from_color(color: &Rgb) -> Self {
        let rgb = color.to_rgb();
        Self::from_rgb(&rgb)
    }
}
impl From<Rgb> for Hsv {
    #[inline]
    fn from(color: Rgb) -> Self {
        Self::from_color(&color)
    }
}

impl FromColor<Hsv> for Hsv {
    #[inline]
    fn from_color(color: &Self) -> Self {
        *color
    }
}

impl FromColor<Hsv> for Rgb {
    #[inline]
    fn from_color(color: &Hsv) -> Self {
        let rgb = color.to_rgb();
        Self::from_rgb(&rgb)
    }
}
impl From<Hsv> for Rgb {
    #[inline]
    fn from(color: Hsv) -> Self {
        Self::from_color(&color)
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
