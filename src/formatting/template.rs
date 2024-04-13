use super::formatter::{new_formatter, Formatter};
use super::{parse, FormatError, Fragment, Values};
use crate::config::SharedConfig;
use crate::errors::*;

use std::str::FromStr;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct FormatTemplate(Arc<[TokenList]>);

impl Default for FormatTemplate {
    fn default() -> Self {
        Self(Arc::new([]))
    }
}

#[derive(Debug)]
pub struct TokenList(pub Vec<Token>);

#[derive(Debug)]
pub enum Token {
    Text(String),
    Recursive(FormatTemplate),
    Placeholder {
        name: String,
        formatter: Option<Box<dyn Formatter>>,
    },
    Icon {
        name: String,
    },
}

impl FormatTemplate {
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.iter().any(|token_list| {
            token_list.0.iter().any(|token| match token {
                Token::Placeholder { name, .. } => name == key,
                Token::Recursive(rec) => rec.contains_key(key),
                _ => false,
            })
        })
    }

    pub fn render(
        &self,
        values: &Values,
        config: &SharedConfig,
    ) -> Result<Vec<Fragment>, FormatError> {
        for (i, token_list) in self.0.iter().enumerate() {
            match token_list.render(values, config) {
                Ok(res) => return Ok(res),
                Err(
                    FormatError::PlaceholderNotFound(_)
                    | FormatError::IncompatibleFormatter { .. }
                    | FormatError::NumberOutOfRange(_),
                ) if i != self.0.len() - 1 => (),
                Err(e) => return Err(e),
            }
        }
        Ok(Vec::new())
    }

    pub fn init_intervals(&self, intervals: &mut Vec<u64>) {
        for tl in self.0.iter() {
            for t in &tl.0 {
                match t {
                    Token::Recursive(r) => r.init_intervals(intervals),
                    Token::Placeholder {
                        formatter: Some(f), ..
                    } => {
                        if let Some(i) = f.interval() {
                            intervals.push(i.as_millis() as u64);
                        }
                    }
                    _ => (),
                }
            }
        }
    }
}

impl TokenList {
    pub fn render(
        &self,
        values: &Values,
        config: &SharedConfig,
    ) -> Result<Vec<Fragment>, FormatError> {
        let mut retval = Vec::new();
        let mut cur = Fragment::default();
        for token in &self.0 {
            match token {
                Token::Text(text) => {
                    if cur.metadata.is_default() {
                        cur.text.push_str(text);
                    } else {
                        if !cur.text.is_empty() {
                            retval.push(cur);
                        }
                        cur = text.clone().into();
                    }
                }
                Token::Recursive(rec) => {
                    if !cur.text.is_empty() {
                        retval.push(cur);
                    }
                    retval.extend(rec.render(values, config)?);
                    cur = retval.pop().unwrap_or_default();
                }
                Token::Placeholder { name, formatter } => {
                    let value = values
                        .get(name.as_str())
                        .ok_or_else(|| FormatError::PlaceholderNotFound(name.into()))?;
                    let formatter = formatter
                        .as_ref()
                        .map(Box::as_ref)
                        .unwrap_or_else(|| value.default_formatter());
                    let formatted = formatter.format(&value.inner, config)?;
                    if value.metadata == cur.metadata {
                        cur.text.push_str(&formatted);
                    } else {
                        if !cur.text.is_empty() {
                            retval.push(cur);
                        }
                        cur = Fragment {
                            text: formatted,
                            metadata: value.metadata,
                        };
                    }
                }
                Token::Icon { name } => {
                    let icon = config.get_icon(name, None)?;
                    if cur.metadata.is_default() {
                        cur.text.push_str(&icon);
                    } else {
                        if !cur.text.is_empty() {
                            retval.push(cur);
                        }
                        cur = icon.into();
                    }
                }
            }
        }

        if !cur.text.is_empty() {
            retval.push(cur);
        }

        Ok(retval)
    }
}

impl FromStr for FormatTemplate {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        parse::parse_full(s)
            .and_then(TryInto::try_into)
            .error("Incorrect format template")
    }
}

impl TryFrom<parse::FormatTemplate<'_>> for FormatTemplate {
    type Error = Error;

    fn try_from(value: parse::FormatTemplate) -> Result<Self, Self::Error> {
        value
            .0
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Arc<[_]>>>()
            .map(Self)
    }
}

impl TryFrom<parse::TokenList<'_>> for TokenList {
    type Error = Error;

    fn try_from(value: parse::TokenList) -> Result<Self, Self::Error> {
        value
            .0
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>>>()
            .map(Self)
    }
}

impl TryFrom<parse::Token<'_>> for Token {
    type Error = Error;

    fn try_from(value: parse::Token) -> Result<Self, Self::Error> {
        Ok(match value {
            parse::Token::Text(text) => Self::Text(text),
            parse::Token::Placeholder(placeholder) => Self::Placeholder {
                name: placeholder.name.to_owned(),
                formatter: placeholder
                    .formatter
                    .map(|fmt| new_formatter(fmt.name, &fmt.args))
                    .transpose()?,
            },
            parse::Token::Icon(icon) => Self::Icon {
                name: icon.to_owned(),
            },
            parse::Token::Recursive(rec) => Self::Recursive(rec.try_into()?),
        })
    }
}
