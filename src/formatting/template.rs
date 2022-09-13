use super::formatter::{new_formatter, Formatter};
use super::{Fragment, Values};
use crate::config::SharedConfig;
use crate::errors::*;

use std::iter::Peekable;
use std::str::FromStr;

#[derive(Debug)]
pub struct FormatTemplate(pub Vec<TokenList>);

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

    pub fn render(&self, values: &Values, config: &SharedConfig) -> Result<Vec<Fragment>> {
        for (i, token_list) in self.0.iter().enumerate() {
            match token_list.render(values, config) {
                Ok(res) => return Ok(res),
                Err(e) if e.kind != ErrorKind::Format => return Err(e),
                Err(e) if i == self.0.len() - 1 => return Err(e),
                _ => (),
            }
        }
        Ok(Vec::new())
    }

    pub fn init_intervals(&self, intervals: &mut Vec<u64>) {
        for tl in &self.0 {
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
    pub fn render(&self, values: &Values, config: &SharedConfig) -> Result<Vec<Fragment>> {
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
                        .or_format_error(|| format!("Placeholder '{}' not found", name))?;
                    let formatter = formatter
                        .as_ref()
                        .map(Box::as_ref)
                        .unwrap_or_else(|| value.default_formatter());
                    let formatted = formatter.format(&value.inner)?;
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
                    let icon = config
                        .get_icon(name)
                        .or_format_error(|| format!("Icon '{}' not found", name))?;
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
        let mut it = s.chars().chain(std::iter::once('}')).peekable();
        let template = read_format_template(&mut it)?;
        if it.next().is_some() {
            Err(Error::new("Unexpected '}'"))
        } else {
            Ok(template)
        }
    }
}

fn read_format_template(it: &mut Peekable<impl Iterator<Item = char>>) -> Result<FormatTemplate> {
    let mut token_lists = Vec::new();
    let mut cur_list = Vec::new();
    loop {
        match *it.peek().error("Missing '}'")? {
            '{' => {
                let _ = it.next();
                cur_list.push(Token::Recursive(read_format_template(it)?));
            }
            '}' => {
                let _ = it.next();
                token_lists.push(TokenList(cur_list));
                return Ok(FormatTemplate(token_lists));
            }
            '|' => {
                let _ = it.next();
                token_lists.push(TokenList(cur_list));
                cur_list = Vec::new();
            }
            '$' => {
                let _ = it.next();
                let name = read_ident(it);
                let formatter = match it.peek() {
                    Some('.') => {
                        let _ = it.next();
                        Some(new_formatter(&read_formatter(it)?, &read_args(it)?)?)
                    }
                    _ => None,
                };
                cur_list.push(Token::Placeholder { name, formatter });
            }
            '^' => {
                let _ = it.next();
                if !consume_exact(it, "icon_") {
                    return Err(Error::new("^ should be followed by 'icon_<name>'"));
                }
                let name = read_ident(it);
                cur_list.push(Token::Icon { name });
            }
            _ => {
                cur_list.push(Token::Text(read_text(it)));
            }
        }
    }
}

fn read_text(it: &mut Peekable<impl Iterator<Item = char>>) -> String {
    let mut retval = String::new();
    let mut escaped = false;
    while let Some(&c) = it.peek() {
        if escaped {
            escaped = false;
            retval.push(c);
            let _ = it.next();
            continue;
        }
        match c {
            '\\' => {
                let _ = it.next();
                escaped = true;
            }
            '{' | '}' | '$' | '^' | '|' => break,
            x => {
                let _ = it.next();
                retval.push(x);
            }
        }
    }
    retval
}

fn read_ident(it: &mut Peekable<impl Iterator<Item = char>>) -> String {
    let mut retval = String::new();
    while let Some(&c) = it.peek() {
        match c {
            x if !x.is_alphanumeric() && x != '_' => break,
            x => {
                let _ = it.next();
                retval.push(x);
            }
        }
    }
    retval
}

fn read_formatter(it: &mut impl Iterator<Item = char>) -> Result<String> {
    let mut retval = String::new();
    for c in it {
        match c {
            '(' => return Ok(retval),
            x => retval.push(x),
        }
    }
    Err(Error::new("Missing '('"))
}

fn read_args(it: &mut impl Iterator<Item = char>) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut cur_arg = String::new();
    let mut escaped = false;
    for c in it {
        if escaped {
            escaped = false;
            cur_arg.push(c);
            continue;
        }
        match c {
            '\\' => escaped = true,
            ',' => {
                args.push(cur_arg);
                cur_arg = String::new();
            }
            ')' => {
                if !cur_arg.is_empty() || !args.is_empty() {
                    args.push(cur_arg);
                }
                return Ok(args);
            }
            x => cur_arg.push(x),
        }
    }
    Err(Error::new("Missing ')'"))
}

fn consume_exact(it: &mut impl Iterator<Item = char>, tag: &str) -> bool {
    for c in tag.chars() {
        if it.next() != Some(c) {
            return false;
        }
    }
    true
}
