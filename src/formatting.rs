//! # Formatting system
//!
//! Many blocks have a `format` configuration option, which allows to heavily customize the block's
//! appearance. In short, each block with `format` option provides a set of values, which are
//! displayed according to `format`. `format`'s value is just a text with embeded variables.
//! Simialrly to PHP and shell, variable name must start with a `$`:
//! `this is a variable: -> $var <-`.
//!
//! Also, format strings can embed icons. For example, `^icon_ping` in `" ^icon_ping $ping "` gets
//! substituted with a "ping" icon from your icon set. For a complete list of icons, see
//! [this](https://github.com/greshake/i3status-rust/blob/master/doc/themes.md#available-icon-overrides).
//!
//! # Types
//!
//! The allowed types of variables are:
//!
//! Type                      | Default formatter
//! --------------------------|------------------
//! Text                      | `str`
//! Number                    | `eng`
//! [Flag](#how-to-use-flags) | N/A
//!
//! # Formatters
//!
//! A formatter is something that converts a value into a text. Because there are many ways to do
//! this, a number of formatters is available. Formatter can be specified using the syntax similar
//! to method calls in many programming languages: `<variable>.<formatter>(<args>)`. For example:
//! `$title.str(min_w:10, max_w:20)`.
//!
//! ## `str` - Format text
//!
//! Argument               | Description                                       |Default value
//! -----------------------|---------------------------------------------------|-------------
//! `min_width` or `min_w` | if text is shorter it will be padded using spaces | `0`
//! `max_width` or `max_w` | if text is longer it will be truncated            | Infinity
//!
//! ## `rot-str` - Rotating text
//!
//! Argument               | Description                                                                |Default value
//! -----------------------|----------------------------------------------------------------------------|-------------
//! `width` or 'w'         | if text is shorter it will be padded using spaces                          | `15`
//! `interval`             | if text is longer than `width` it will be rotated every `interval` seconds | `0.5`
//!
//! ## `eng` - Format numbers using engineering notation
//!
//! Argument        | Description                                                                                      |Default value
//! ----------------|--------------------------------------------------------------------------------------------------|-------------
//! `width` or `w`  | the resulting text will be at least `width` characters long                                      | `3`
//! `unit` or `u`   | some values have a [unit](unit::Unit), and it is possible to convert them by setting this option | N/A
//! `hide_unit`     | hide the unit symbol                                                                             | `false`
//! `unit_space`    | have a whitespace before unit symbol                                                             | `false`
//! `prefix` or `p` | specifiy this argument if you want to set the minimal [SI prefix](prefix::Prefix)                | N/A
//! `hide_prefix`   | hide the prefix symbol                                                                           | `false`
//! `prefix_space`  | have a whitespace before prefix symbol                                                           | `false`
//! `force_prefix`  | force the prefix value instead of setting a "minimal prefix"                                     | `false`
//!
//! ## `bar` - Display numbers as progress bars
//!
//! Argument               | Description                                                                     |Default value
//! -----------------------|---------------------------------------------------------------------------------|-------------
//! `width` or `w`         | the width of the bar (in characters)                                            | `5`
//! `max_value`            | which value is treated as "full". For example, for battery level `100` is full. | `100`
//!
//! # Handling missing placeholders and incorrect types
//!
//! Some blocks allow missing placeholders, for example [bluetooth](crate::blocks::bluetooth)'s
//! "percentage" may be absent if the device is not supported. To handle such cases it is possible
//! to queue multiple formats together by using `|` symbol: `<something that can fail>|<otherwise
//! try this>|<or this>`.
//!
//! In addition, formats can be recursive. To set a format inside of another format, place it
//! inside of `{}`. For example, in `Percentage: {$percentage|N/A}` the text "Percentage: " will be
//! always displayed, followed by the actual percentage or "N/A" in case percentage is not
//! available. This example does exactly the same thing as `Percentage: $percentage|Percentage: N/A`
//!
//! # How to use flags
//!
//! Some blocks provide flags, which can be used to change the format based on some critera. For
//! example, [taskwarrior](crate::blocks::taskwarrior) defines `done` if the count is zero. In
//! general, flags are used in this way:
//!
//! ```text
//! $a{a is set}|$b$c{b and c are set}|${b|c}{b or c is set}|neither flag is set
//! ```

pub mod config;
pub mod formatter;
pub mod parse;
pub mod prefix;
pub mod scheduling;
pub mod template;
pub mod unit;
pub mod value;

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::SharedConfig;
use crate::errors::*;
use template::FormatTemplate;
use value::Value;

pub type Values = HashMap<Cow<'static, str>, Value>;

pub type Format = Arc<FormatInner>;

#[derive(Debug)]
pub struct FormatInner {
    full: FormatTemplate,
    short: FormatTemplate,
    intervals: Vec<u64>,
}

impl FormatInner {
    pub fn contains_key(&self, key: &str) -> bool {
        self.full.contains_key(key) || self.short.contains_key(key)
    }

    pub fn intervals(&self) -> Vec<u64> {
        self.intervals.clone()
    }

    pub fn render(
        &self,
        values: &Values,
        config: &SharedConfig,
    ) -> Result<(Vec<Fragment>, Vec<Fragment>)> {
        let full = self
            .full
            .render(values, config)
            .error("Failed to render full text")?;
        let short = self
            .short
            .render(values, config)
            .error("Failed to render short text")?;
        Ok((full, short))
    }
}

#[derive(Debug, Default, Clone)]
pub struct Fragment {
    pub text: String,
    pub metadata: Metadata,
}

impl From<String> for Fragment {
    fn from(text: String) -> Self {
        Self {
            text,
            metadata: Default::default(),
        }
    }
}

impl Fragment {
    pub fn formated_text(&self) -> String {
        match (self.metadata.italic, self.metadata.underline) {
            (true, true) => format!("<i><u>{}</u></i>", self.text),
            (false, true) => format!("<u>{}</u>", self.text),
            (true, false) => format!("<i>{}</i>", self.text),
            (false, false) => self.text.clone(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Metadata {
    pub instance: Option<&'static str>,
    pub underline: bool,
    pub italic: bool,
}

impl Metadata {
    pub fn is_default(&self) -> bool {
        *self == Default::default()
    }
}
