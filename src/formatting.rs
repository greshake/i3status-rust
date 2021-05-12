pub mod placeholder;
pub mod prefix;
pub mod unit;
pub mod value;

use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;

use serde::de::{MapAccess, Visitor};
use serde::{de, Deserialize, Deserializer};

use crate::errors::*;
use placeholder::Placeholder;
use value::Value;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Text(String),
    Var(Placeholder),
}

fn unexpected_token<T>(token: char) -> Result<T> {
    Err(ConfigurationError(
        format!(
            "failed to parse formatting string: unexpected token '{}'",
            token
        ),
        String::new(),
    ))
}

#[derive(Debug, Clone, Default)]
pub struct FormatTemplate {
    full: Option<Vec<Token>>,
    short: Option<Vec<Token>>,
}

impl FormatTemplate {
    /// Whether the format string contains given placeholder
    pub fn contains(&self, var: &str) -> bool {
        Self::format_contains(&self.full, var) || Self::format_contains(&self.short, var)
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

    pub fn new(full: &str, short: Option<&str>) -> Result<Self> {
        Self::from_options(Some(full), short)
    }

    pub fn from_options(full: Option<&str>, short: Option<&str>) -> Result<Self> {
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

    pub fn with_default(mut self, default: &str) -> Result<Self> {
        if self.full.is_none() {
            self.full = Some(Self::tokens_from_string(default)?);
        }
        Ok(self)
    }

    fn tokens_from_string(s: &str) -> Result<Vec<Token>> {
        let mut tokens = vec![];

        let mut text_buf = String::new();
        let mut var_buf = String::new();
        let mut inside_var = false;

        let mut current_buf = &mut text_buf;

        for c in s.chars() {
            match c {
                '{' => {
                    if inside_var {
                        return unexpected_token(c);
                    }
                    if !text_buf.is_empty() {
                        tokens.push(Token::Text(text_buf.clone()));
                        text_buf.clear();
                    }
                    current_buf = &mut var_buf;
                    inside_var = true;
                }
                '}' => {
                    if !inside_var {
                        return unexpected_token(c);
                    }
                    tokens.push(Token::Var(var_buf.as_str().try_into()?));
                    var_buf.clear();
                    current_buf = &mut text_buf;
                    inside_var = false;
                }
                x => current_buf.push(x),
            }
        }
        if inside_var {
            return Err(ConfigurationError(
                "failed to parse formatting string: missing '}'".to_string(),
                "".to_string(),
            ));
        }
        if !text_buf.is_empty() {
            tokens.push(Token::Text(text_buf.clone()));
        }

        Ok(tokens)
    }

    pub fn render(&self, vars: &HashMap<&str, Value>) -> Result<(String, Option<String>)> {
        let full = match &self.full {
            Some(tokens) => Self::render_tokens(tokens, vars)?,
            None => String::new(),
        };
        let short = match &self.short {
            Some(short) => Some(Self::render_tokens(short, vars)?),
            None => None,
        };
        Ok((full, short))
    }

    fn render_tokens(tokens: &[Token], vars: &HashMap<&str, Value>) -> Result<String> {
        let mut rendered = String::new();

        for token in tokens {
            match token {
                Token::Text(text) => rendered.push_str(&text),
                Token::Var(var) => rendered.push_str(
                    &vars
                        .get(&*var.name)
                        .internal_error(
                            "util",
                            &format!("Unknown placeholder in format string: {}", var.name),
                        )?
                        .format(&var)?,
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
                FormatTemplate::new(full, None).map_err(|e| de::Error::custom(e.to_string()))
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

                FormatTemplate::from_options(full.as_deref(), short.as_deref())
                    .map_err(|e| de::Error::custom(e.to_string()))
            }
        }

        deserializer.deserialize_any(FormatTemplateVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prefix::Prefix;
    use unit::Unit;

    #[test]
    fn from_string() {
        let ft = FormatTemplate::new(
            "some text {var} var again {var*_}{new_var:3} {bar:2#100} {freq;1}.",
            None,
        );
        assert!(ft.is_ok());

        let mut tokens = ft.unwrap().full.unwrap().into_iter();
        assert_eq!(
            tokens.next().unwrap(),
            Token::Text("some text ".to_string())
        );
        assert_eq!(
            tokens.next().unwrap(),
            Token::Var(Placeholder {
                name: "var".to_string(),
                min_width: None,
                max_width: None,
                pad_with: None,
                min_prefix: None,
                unit: None,
                unit_hidden: false,
                bar_max_value: None
            })
        );
        assert_eq!(
            tokens.next().unwrap(),
            Token::Text(" var again ".to_string())
        );
        assert_eq!(
            tokens.next().unwrap(),
            Token::Var(Placeholder {
                name: "var".to_string(),
                min_width: None,
                max_width: None,
                pad_with: None,
                min_prefix: None,
                unit: Some(Unit::None),
                unit_hidden: true,
                bar_max_value: None
            })
        );
        assert_eq!(
            tokens.next().unwrap(),
            Token::Var(Placeholder {
                name: "new_var".to_string(),
                min_width: Some(3),
                max_width: None,
                pad_with: None,
                min_prefix: None,
                unit: None,
                unit_hidden: false,
                bar_max_value: None
            })
        );
        assert_eq!(tokens.next().unwrap(), Token::Text(" ".to_string()));
        assert_eq!(
            tokens.next().unwrap(),
            Token::Var(Placeholder {
                name: "bar".to_string(),
                min_width: Some(2),
                max_width: None,
                pad_with: None,
                min_prefix: None,
                unit: None,
                unit_hidden: false,
                bar_max_value: Some(100.)
            })
        );
        assert_eq!(tokens.next().unwrap(), Token::Text(" ".to_string()));
        assert_eq!(
            tokens.next().unwrap(),
            Token::Var(Placeholder {
                name: "freq".to_string(),
                min_width: None,
                max_width: None,
                pad_with: None,
                min_prefix: Some(Prefix::One),
                unit: None,
                unit_hidden: false,
                bar_max_value: None
            })
        );
        assert_eq!(tokens.next().unwrap(), Token::Text(".".to_string()));
        assert!(matches!(tokens.next(), None));
    }

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
