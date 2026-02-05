use regex::Regex;

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::errors::*;
use crate::themes::color::{Color, Rgba};

make_log_macro!(debug, "xresources");

#[cfg(not(test))]
use std::{env, path::PathBuf};

#[cfg(not(test))]
fn read_xresources() -> std::io::Result<String> {
    let home = env::var("HOME").map_err(|_| std::io::Error::other("HOME env var was not set"))?;
    let xresources = PathBuf::from(home + "/.Xresources");
    debug!(".Xresources @ {:?}", xresources);
    std::fs::read_to_string(xresources)
}

#[cfg(test)]
use tests::read_xresources;

// Defines: static XORG_COLORS: ::phf::Map<&'static str, Color>
include!(concat!(env!("OUT_DIR"), "/xorg_rgb_codegen.rs"));

static COLORS: LazyLock<Result<HashMap<String, Result<Color>>>> = LazyLock::new(|| {
    let regex =
        Regex::new(r"^(\s*\*|#define\s+)(?<name>[^:\s]+)\s*:?\s*(?<color>[^\n]*).*$").unwrap();
    let content = read_xresources().error("could not read .Xresources")?;
    debug!(".Xresources content:\n{}", content);
    let mut color_map: HashMap<String, Result<Color>> = HashMap::new();
    for line in content.lines() {
        let Some(caps) = regex.captures(line) else {
            continue;
        };

        let name = caps["name"].to_lowercase();
        let color_str = caps["color"].trim();
        let hex_err_msg = || format!("'{color_str}' is not a valid RGB color");
        let color = if let Some(color_hex) = color_str.strip_prefix('#') {
            if color_hex.len() == 3 {
                let r = u8::from_str_radix(&color_hex[0..1], 16).or_error(hex_err_msg)? << 4;
                let g = u8::from_str_radix(&color_hex[1..2], 16).or_error(hex_err_msg)? << 4;
                let b = u8::from_str_radix(&color_hex[2..3], 16).or_error(hex_err_msg)? << 4;
                Ok(Color::Rgba(Rgba { r, g, b, a: 255 }))
            } else if color_hex.len() == 6 {
                Ok(Color::Rgba(Rgba::from_hex(
                    (u32::from_str_radix(color_hex, 16).or_error(hex_err_msg)? << 8) + 255,
                )))
            } else if color_hex.len() == 9 {
                let r = (u16::from_str_radix(&color_hex[0..3], 16).or_error(hex_err_msg)? as f64
                    / 4095.0
                    * 255.0)
                    .round() as u8;
                let g = (u16::from_str_radix(&color_hex[3..6], 16).or_error(hex_err_msg)? as f64
                    / 4095.0
                    * 255.0)
                    .round() as u8;
                let b = (u16::from_str_radix(&color_hex[6..9], 16).or_error(hex_err_msg)? as f64
                    / 4095.0
                    * 255.0)
                    .round() as u8;
                Ok(Color::Rgba(Rgba { r, g, b, a: 255 }))
            } else if color_hex.len() == 12 {
                let r = (u16::from_str_radix(&color_hex[0..4], 16).or_error(hex_err_msg)? as f64
                    / 65535.0
                    * 255.0)
                    .round() as u8;
                let g = (u16::from_str_radix(&color_hex[4..8], 16).or_error(hex_err_msg)? as f64
                    / 65535.0
                    * 255.0)
                    .round() as u8;
                let b = (u16::from_str_radix(&color_hex[8..12], 16).or_error(hex_err_msg)? as f64
                    / 65535.0
                    * 255.0)
                    .round() as u8;
                Ok(Color::Rgba(Rgba { r, g, b, a: 255 }))
            } else {
                Err(Error::new(format!(
                    "color '{name}' has an invalid length hex code: '{color_str}'"
                )))
            }
        } else if let Some((color_space, color_values)) = color_str.split_once(":") {
            let mut color_value_split = color_values.split("/");
            let cv1 = color_value_split
                .next()
                .error("Not enough / separated values")?;
            let cv2 = color_value_split
                .next()
                .error("Not enough / separated values")?;
            let cv3 = color_value_split
                .next()
                .error("Not enough / separated values")?;
            match color_space {
                "rgb" => {
                    let r = ((u16::from_str_radix(cv1, 16).or_error(hex_err_msg)? as f64)
                        / (((1 << (4 * cv1.len())) - 1) as f64)
                        * 255.0)
                        .round() as u8;
                    let g = ((u16::from_str_radix(cv2, 16).or_error(hex_err_msg)? as f64)
                        / (((1 << (4 * cv2.len())) - 1) as f64)
                        * 255.0)
                        .round() as u8;
                    let b = ((u16::from_str_radix(cv3, 16).or_error(hex_err_msg)? as f64)
                        / (((1 << (4 * cv3.len())) - 1) as f64)
                        * 255.0)
                        .round() as u8;
                    Ok(Color::Rgba(Rgba { r, g, b, a: 255 }))
                }
                "rgbi" => {
                    let r =
                        (255.0 * cv1.parse::<f64>().error("red value is not a valid float")?) as u8;
                    let g = (255.0
                        * cv2
                            .parse::<f64>()
                            .error("green value is not a valid float")?)
                        as u8;
                    let b = (255.0
                        * cv3
                            .parse::<f64>()
                            .error("blue value is not a valid float")?)
                        as u8;

                    Ok(Color::Rgba(Rgba { r, g, b, a: 255 }))
                }
                "CIEXYZ" | "CIEuvY" | "CIExyY" | "CIELab" | "CIELuv" | "TekHVC" => Err(Error::new(
                    format!("color '{name}' is in an unimplemented color space '{color_space}'"),
                )),
                _ => Err(Error::new(format!(
                    "color '{name}' is in an unrecognized color space '{color_space}'"
                ))),
            }
        } else {
            let color_str = color_str.to_lowercase();
            if let Some(res) = color_map.get(&color_str) {
                res.clone()
            } else if let Some(color) = XORG_COLORS.get(&color_str) {
                Ok(*color)
            } else {
                Err(Error::new(format!(
                    "unable to resolve '{color_str}' for color '{name}'"
                )))
            }
        };
        color_map.insert(name, color);
    }

    Ok(color_map)
});

pub fn get_color(name: &str) -> Result<Color> {
    let name = name.to_lowercase();
    COLORS.as_ref().map_err(Clone::clone).and_then(|cmap| {
        if let Some(res) = cmap.get(&name) {
            res.clone()
        } else if let Some(color) = XORG_COLORS.get(&name) {
            Ok(*color)
        } else {
            Err(Error::new(format!(
                "color '{name}' not defined in ~/.Xresources"
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn read_xresources() -> std::io::Result<String> {
        static XRESOURCES: &str = include_str!("../../testdata/xresources.txt");
        Ok(XRESOURCES.to_string())
    }

    #[test]
    fn test_xorg_defined_color() {
        assert_eq!(
            get_color("lemonchiffon").unwrap(),
            "#fffacd".parse::<Color>().unwrap()
        );

        assert_eq!(
            get_color("LemonChiffon").unwrap(),
            "#fffacd".parse::<Color>().unwrap()
        );

        assert_eq!(
            get_color("Lemon Chiffon").unwrap(),
            "#fffacd".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_color_ref_to_xorg_defined_color() {
        assert_eq!(
            get_color("color0").unwrap(),
            "#fffacd".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_color_ref_to_xresources_defined_color() {
        assert_eq!(
            get_color("color1").unwrap(),
            "#dc322f".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_color_ref_to_xorg_redefined_colors() {
        assert_eq!(
            get_color("color2").unwrap(),
            "#859900".parse::<Color>().unwrap()
        );

        assert_eq!(
            get_color("color3").unwrap(),
            "#b58900".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_legacy_12_bit_rgb_color() {
        assert_eq!(
            get_color("color4").unwrap(),
            "#2080d0".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_legacy_24_bit_rgb_color() {
        assert_eq!(
            get_color("color5").unwrap(),
            "#d33682".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_legacy_36_bit_rgb_color() {
        assert_eq!(
            get_color("color6").unwrap(),
            "#2aa198".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_legacy_48_bit_rgb_color() {
        assert_eq!(
            get_color("color7").unwrap(),
            "#ede8d5".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_12_bit_rgb_color() {
        assert_eq!(
            get_color("color8").unwrap(),
            "#cc4411".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_24_bit_rgb_color() {
        assert_eq!(
            get_color("color9").unwrap(),
            "#002b36".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_36_bit_rgb_color() {
        assert_eq!(
            get_color("color10").unwrap(),
            "#586e76".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_48_bit_rgb_color() {
        assert_eq!(
            get_color("color11").unwrap(),
            "#829496".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_rgbi_color() {
        assert_eq!(
            get_color("color12").unwrap(),
            "#ddb787".parse::<Color>().unwrap()
        );
    }

    #[test]
    fn test_invalid_legacy_rgb_color() {
        let color13 = get_color("color13");
        assert!(matches!(
            color13,
            Err(Error {
                message:
                    Some(msg) ,
                cause: None,
            })
            if msg == "color 'color13' has an invalid length hex code: '#6c71c4ff'"
        ));
    }

    #[test]
    fn test_color_in_unimplemented_colorspace() {
        let color14 = get_color("color14");
        assert!(matches!(
            color14,
            Err(Error {
                message: Some(msg),
                cause: None,
            })  if msg == "color 'color14' is in an unimplemented color space 'CIEXYZ'"
        ));
    }

    #[test]
    fn test_color_in_unknown_colorspace() {
        let color15 = get_color("color15");
        assert!(matches!(
            color15,
            Err(Error {
                message: Some(msg),
                cause: None,
            })  if msg == "color 'color15' is in an unrecognized color space 'Unknown'"
        ));
    }

    #[test]
    fn test_undefined_color() {
        let not_a_color = get_color("not_a_color");
        assert!(matches!(
            not_a_color,
            Err(Error {
                message: Some(msg),
                cause: None,
            })  if msg == "color 'not_a_color' not defined in ~/.Xresources"
        ));
    }
}
