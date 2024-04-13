use log::debug;
use regex::Regex;

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::errors::*;

#[cfg(not(test))]
use std::{env, path::PathBuf};

#[cfg(not(test))]
fn read_xresources() -> std::io::Result<String> {
    use std::io::{Error, ErrorKind};
    let home =
        env::var("HOME").map_err(|_| Error::new(ErrorKind::Other, "HOME env var was not set"))?;
    let xresources = PathBuf::from(home + "/.Xresources");
    debug!(".Xresources @ {:?}", xresources);
    std::fs::read_to_string(xresources)
}

#[cfg(test)]
use tests::read_xresources;

static COLOR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*\*(?<name>[^: ]+)\s*:\s*(?<color>#[a-f0-9]{6,8}).*$").unwrap()
});

static COLORS: LazyLock<Result<HashMap<String, String>, Error>> = LazyLock::new(|| {
    let content = read_xresources().error("could not read .Xresources")?;
    debug!(".Xresources content:\n{}", content);
    Ok(HashMap::from_iter(content.lines().filter_map(|line| {
        COLOR_REGEX
            .captures(line)
            .map(|caps| (caps["name"].to_string(), caps["color"].to_string()))
    })))
});

pub fn get_color(name: &str) -> Result<Option<&String>, Error> {
    COLORS
        .as_ref()
        .map(|cmap| cmap.get(name))
        .map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Result;

    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn read_xresources() -> Result<String> {
        static XRESOURCES: &str = "\
        ! this is a comment\n\
        \n\
        *color4 : #feedda\n\
    \n\
        *background: #ee33aa99\n\
        ";
        Ok(XRESOURCES.to_string())
    }

    #[test]
    fn test_reading_colors() {
        let colors = COLORS.as_ref().unwrap();
        assert_eq!(colors.get("color4"), Some(&"#feedda".to_string()));
        assert_eq!(colors.get("background"), Some(&"#ee33aa99".to_string()));
        assert_eq!(2, colors.len());
    }

    #[test]
    fn test_deserializing_xcolors() {
        use super::super::color::*;
        let mut parsed_color = "x:color4".parse::<Color>().unwrap();
        assert_eq!(
            parsed_color,
            Color::Rgba(Rgba {
                r: 254,
                g: 237,
                b: 218,
                a: 255
            })
        );
        parsed_color = "x:background".parse::<Color>().unwrap();
        assert_eq!(
            parsed_color,
            Color::Rgba(Rgba {
                r: 238,
                g: 51,
                b: 170,
                a: 153,
            })
        );
    }
}
