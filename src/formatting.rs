pub mod placeholder;
pub mod prefix;
pub mod unit;
pub mod value;

use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;

use serde::de::{MapAccess, Visitor};
use serde::{de, Deserialize, Deserializer};

use crate::errors::*;
use placeholder::unexpected_token;
use placeholder::Placeholder;
use value::Value;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Text(String),
    Var(Placeholder),
}

#[derive(Debug, Default, Clone)]
pub struct FormatTemplate {
    full: Option<Vec<Token>>,
    short: Option<Vec<Token>>,
}

pub trait FormatMapKey: Borrow<str> + Eq + Hash {}
impl<T> FormatMapKey for T where T: Borrow<str> + Eq + Hash {}

impl FormatTemplate {
    pub fn new(full: &str, short: Option<&str>) -> Result<Self> {
        Self::new_opt(Some(full), short)
    }

    pub fn new_opt(full: Option<&str>, short: Option<&str>) -> Result<Self> {
        let full = match full {
            Some(full) => Some(Self::tokens_from_string(full)?),
            None => None,
        };
        let short = match short {
            Some(short) => Some(Self::tokens_from_string(short)?),
            None => None,
        };
        Ok(Self { full, short })
    }

    /// Initialize `full` field if it is `None`
    pub fn with_default(mut self, default_full: &str) -> Result<Self> {
        if self.full.is_none() {
            self.full = Some(Self::tokens_from_string(default_full)?);
        }
        Ok(self)
    }

    /// Whether the format string contains a given placeholder
    pub fn contains(&self, var: &str) -> bool {
        Self::format_contains(&self.full, var) || Self::format_contains(&self.short, var)
    }

    pub fn has_tokens(&self) -> bool {
        !self.full.as_ref().map(Vec::is_empty).unwrap_or(true)
            || !self.short.as_ref().map(Vec::is_empty).unwrap_or(true)
    }

    fn format_contains(format: &Option<Vec<Token>>, var: &str) -> bool {
        if let Some(tokens) = format {
            for token in tokens {
                if let Token::Var(ref placeholder) = token {
                    if placeholder.name == var {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn tokens_from_string(mut s: &str) -> Result<Vec<Token>> {
        let mut tokens = vec![];

        // Push text into tokens vector. Check the text for correctness and don't push empty strings
        let push_text = |tokens: &mut Vec<Token>, x: &str| {
            if x.contains('{') {
                unexpected_token('{')
            } else if x.contains('}') {
                unexpected_token('}')
            } else if !x.is_empty() {
                tokens.push(Token::Text(x.to_string()));
                Ok(())
            } else {
                Ok(())
            }
        };

        while !s.is_empty() {
            // Split `"text {key:1} {key}"` into `"text "` and `"key:1} {key}"`
            match s.split_once('{') {
                // No placeholders found -> the whole string is just text
                None => {
                    push_text(&mut tokens, s)?;
                    break;
                }
                // Found placeholder
                Some((before, after)) => {
                    // `before` is just a text
                    push_text(&mut tokens, before)?;
                    // Split `"key:1} {key}"` into `"key:1"` and `" {key}"`
                    match after.split_once('}') {
                        // No matching `}`!
                        None => {
                            return Err(InternalError(
                                "format parser".to_string(),
                                "missing '}'".to_string(),
                                None,
                            ));
                        }
                        // Found the entire placeholder
                        Some((placeholder, rest)) => {
                            // `placeholder.parse()` parses the placeholder's configuration string
                            // (e.g. something like `"key:1;K"`) into `Placeholder` struct. We don't
                            // need to think about that in this code.
                            tokens.push(Token::Var(placeholder.parse()?));
                            s = rest;
                        }
                    }
                }
            }
        }

        Ok(tokens)
    }

    pub fn render(
        &self,
        vars: &HashMap<impl FormatMapKey, Value>,
    ) -> Result<(String, Option<String>)> {
        let full = match &self.full {
            Some(tokens) => Self::render_tokens(tokens, vars)?,
            None => String::new(), // TODO: throw an error that says that it's a bug?
        };
        let short = match &self.short {
            Some(short) => Some(Self::render_tokens(short, vars)?),
            None => None,
        };
        Ok((full, short))
    }

    fn render_tokens(tokens: &[Token], vars: &HashMap<impl FormatMapKey, Value>) -> Result<String> {
        let mut rendered = String::new();
        for token in tokens {
            match token {
                Token::Text(text) => rendered.push_str(text),
                Token::Var(var) => rendered.push_str(
                    &vars
                        .get(&*var.name)
                        .internal_error(
                            "util",
                            &format!("Unknown placeholder in format string: '{}'", var.name),
                        )?
                        .format(var)?,
                ),
            }
        }
        Ok(rendered)
    }
}

impl<'de> Deserialize<'de> for FormatTemplate {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Full,
            Short,
        }

        struct FormatTemplateVisitor;

        impl<'de> Visitor<'de> for FormatTemplateVisitor {
            type Value = FormatTemplate;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("format structure")
            }

            /// Handle configs like:
            ///
            /// ```toml
            /// format = "{layout}"
            /// ```
            fn visit_str<E>(self, full: &str) -> StdResult<FormatTemplate, E>
            where
                E: de::Error,
            {
                FormatTemplate::new(full, None).map_err(de::Error::custom)
            }

            /// Handle configs like:
            ///
            /// ```toml
            /// [block.format]
            /// full = "{layout}"
            /// short = "{layout^2}"
            /// ```
            fn visit_map<V>(self, mut map: V) -> StdResult<FormatTemplate, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut full: Option<String> = None;
                let mut short: Option<String> = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Full => {
                            if full.is_some() {
                                return Err(de::Error::duplicate_field("full"));
                            }
                            full = Some(map.next_value()?);
                        }
                        Field::Short => {
                            if short.is_some() {
                                return Err(de::Error::duplicate_field("short"));
                            }
                            short = Some(map.next_value()?);
                        }
                    }
                }

                FormatTemplate::new_opt(full.as_deref(), short.as_deref())
                    .map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_any(FormatTemplateVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render() {
        let ft = FormatTemplate::new(
            "some text {var} var again {var}{new_var:3} {bar:2#100} {freq;1}.",
            None,
        );
        assert!(ft.is_ok());

        let values = map!(
            "var" => Value::from_string("|var value|".to_string()),
            "new_var" => Value::from_integer(12),
            "bar" => Value::from_integer(25),
            "freq" => Value::from_float(0.01).hertz(),
        );

        assert_eq!(
            ft.unwrap().render(&values).unwrap().0.as_str(),
            "some text |var value| var again |var value| 12 \u{258c}  0.0Hz."
        );
    }

    #[test]
    fn contains() {
        let format = FormatTemplate::new("some text {foo} {bar:1} foobar", None);
        assert!(format.is_ok());
        let format = format.unwrap();
        assert!(format.contains("foo"));
        assert!(format.contains("bar"));
        assert!(!format.contains("foobar"));
        assert!(!format.contains("random string"));
    }
}
