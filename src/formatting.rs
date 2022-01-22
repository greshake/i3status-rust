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
//! Minimal width - if text is shorter it will be paded using spaces | `0`
//! Maximal width - if text is longer it will be truncated | `inf`
//!
//! ## `rot-str` - Rotating text
//!
//! Argument | Default value
//! ---------|--------------
//! Width - if text is shorter it will be paded using spaces | `15`
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
pub mod template;
pub mod unit;
pub mod value;

use smartstring::alias::String;
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

use crate::errors::*;
use crate::Request;
use template::FormatTemplate;
use value::Value;

pub type Values = HashMap<String, Value>;

#[derive(Debug, Clone)]
pub struct Format(Arc<(FormatTemplate, Option<FormatTemplate>)>);

impl Format {
    pub fn run(self, tx: &Sender<Request>, block_id: usize) -> RunningFormat {
        let mut handles = Handles(Vec::new());
        self.0 .0.init(tx, block_id, &mut handles);
        if let Some(short) = &self.0 .1 {
            short.init(tx, block_id, &mut handles);
        }
        RunningFormat(self, handles)
    }

    pub fn run_no_init(self) -> RunningFormat {
        RunningFormat(self, Handles::default())
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0 .0.contains_key(key) || self.0 .1.as_ref().map_or(false, |x| x.contains_key(key))
    }
}

#[derive(Debug, Default)]
pub struct Handles(Vec<JoinHandle<()>>);

impl Drop for Handles {
    fn drop(&mut self) {
        for handle in &self.0 {
            handle.abort();
        }
    }
}

#[derive(Debug)]
pub struct RunningFormat(Format, Handles);

impl RunningFormat {
    pub fn render(&self, vars: &Values) -> Result<(String, Option<String>)> {
        let (full, short) = self.0 .0.as_ref();
        let full = full.render(vars).error("Failed to render full text")?;
        let short = match short {
            Some(short) => Some(short.render(vars).error("Failed to render short text")?),
            None => None,
        };
        Ok((full, short))
    }
}
