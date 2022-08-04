//! # Formatting system
//!
//! Many blocks have a `format` configuration option, which allows to heavily customize the block's
//! appearance. In short, each block with `format` option provides a set of values, which are
//! displayed according to `format`. `format`'s value is just a text with embeded variables.
//! Simialrly to PHP and shell, variable name must start with a `$`:
//! `this is a variable: -> $var <-`.
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
//! to method calls in many programming languages: `<variable>.<formatter>(<args>)`.
//!
//! ## `str` - Format text
//!
//! Argument | Default value
//! ---------|--------------
//! Min width - if text is shorter it will be padded using spaces | `0`
//! Max width - if text is longer it will be truncated | `inf`
//!
//! ## `rot-str` - Rotating text
//!
//! Argument | Default value
//! ---------|--------------
//! Width - if text is shorter it will be padded using spaces | `15`
//! Interval - If text is longer than `width` it will be rotated every `interval` seconds | `1.0`
//!
//! ## `eng` - Format numbers using engineering notation
//!
//! Argument | Default value
//! ---------|--------------
//! Width - the resulting text will be at least `width` characters long | `2`
//! Unit - some values have a [unit](unit::Unit), and it is possible to convert them by setting this option. Perpend this with a space to split unit from number/prefix. Prepend this with a `_` to hide. | `auto`
//! Prefix - specifiy this argument if you want to set the minimal [SI prefix](prefix::Prefix). Prepend this width a space to split prefix from number. Perpend this with a `_` to hide. Perpend this with a `!` to force the prefix. | `auto`
//!
//! ## `bar` - Display numbers as progress bars
//!
//! Argument | Default value
//! ---------|--------------
//! Width - the width of the bar (in characters) | `5`
//! Max value - which value is treated as "full". For example, for battery level `100` is full. | `100`
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
pub mod prefix;
pub mod scheduling;
pub mod template;
pub mod unit;
pub mod value;

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use crate::errors::*;
use crate::widget::State;
use template::FormatTemplate;
use value::Value;

pub type Values = HashMap<Cow<'static, str>, Value>;

#[derive(Debug, Clone)]
pub struct Format {
    inner: Arc<FormatInner>,
}

impl Deref for Format {
    type Target = FormatInner;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

impl Format {
    pub fn intervals(&self) -> Vec<u64> {
        self.inner.intervals.clone()
    }
}

#[derive(Debug)]
pub struct FormatInner {
    full: FormatTemplate,
    short: Option<FormatTemplate>,
    intervals: Vec<u64>,
}

impl FormatInner {
    pub fn contains_key(&self, key: &str) -> bool {
        self.full.contains_key(key) || self.short.as_ref().map_or(false, |x| x.contains_key(key))
    }

    pub fn render(&self, vars: &Values) -> Result<(Vec<Rendered>, Vec<Rendered>)> {
        let full = self.full.render(vars).error("Failed to render full text")?;
        let short = self
            .short
            .as_ref()
            .map(|s| s.render(vars))
            .transpose()
            .error("Failed to render short text")?
            .unwrap_or_default();
        Ok((full, short))
    }
}

#[derive(Debug, Default, Clone)]
pub struct Rendered {
    pub text: String,
    pub metadata: Metadata,
}

impl Rendered {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            metadata: Default::default(),
        }
    }
}

impl From<String> for Rendered {
    fn from(text: String) -> Self {
        Self {
            text,
            metadata: Default::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Metadata {
    pub instance: Option<usize>,
    pub state: Option<State>,
}

impl Metadata {
    pub fn is_default(&self) -> bool {
        *self == Default::default()
    }
}
