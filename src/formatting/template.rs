use super::formatter::{
    new_formatter, Formatter, DEFAULT_FLAG_FORMATTER, DEFAULT_NUMBER_FORMATTER,
    DEFAULT_STRING_FORMATTER,
};
use super::value::ValueInner;
use super::{Rendered, Values};
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
    Var {
        name: String,
        formatter: Option<Box<dyn Formatter + Send + Sync>>,
    },
}

impl FormatTemplate {
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.iter().any(|token_list| {
            token_list.0.iter().any(|token| match token {
                Token::Var { name, .. } => name == key,
                Token::Recursive(rec) => rec.contains_key(key),
                _ => false,
            })
        })
    }

    pub fn render(&self, vars: &Values) -> Result<Vec<Rendered>> {
        for (i, token_list) in self.0.iter().enumerate() {
            match token_list.render(vars) {
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
                    Token::Var {
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
    pub fn render(&self, vars: &Values) -> Result<Vec<Rendered>> {
        let mut retval = Vec::new();
        let mut cur = Rendered::default();
        for token in &self.0 {
            match token {
                Token::Text(text) => {
                    if cur.metadata.is_default() {
                        cur.text.push_str(text);
                    } else {
                        let cur = std::mem::replace(&mut cur, Rendered::new(text.clone()));
                        if !cur.text.is_empty() {
                            retval.push(cur);
                        }
                    }
                }
                Token::Recursive(rec) => {
                    if !cur.text.is_empty() {
                        retval.push(std::mem::take(&mut cur));
                    }
                    retval.extend(rec.render(vars)?);
                    cur = retval.pop().unwrap_or_default();
                }
                Token::Var { name, formatter } => {
                    let var = vars
                        .get(name.as_str())
                        .format_error(format!("Placeholder with name '{}' not found", name))?;
                    let formatter = formatter.as_ref().map(|x| x.as_ref()).unwrap_or_else(|| {
                        match &var.inner {
                            ValueInner::Text(_) | ValueInner::Icon(_) => &DEFAULT_STRING_FORMATTER,
                            ValueInner::Number { .. } => &DEFAULT_NUMBER_FORMATTER,
                            ValueInner::Flag => &DEFAULT_FLAG_FORMATTER,
                        }
                    });
                    let formatted = formatter.format(&var.inner)?;
                    if var.metadata == cur.metadata {
                        cur.text.push_str(&formatted);
                    } else {
                        let cur = std::mem::replace(
                            &mut cur,
                            Rendered {
                                text: formatted,
                                metadata: var.metadata,
                            },
                        );
                        if !cur.text.is_empty() {
                            retval.push(cur);
                        }
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
                let name = read_placeholder_name(it);
                let formatter = match it.peek() {
                    Some('.') => {
                        let _ = it.next();
                        Some(new_formatter(&read_formatter(it)?, &read_args(it)?)?)
                    }
                    _ => None,
                };
                cur_list.push(Token::Var { name, formatter });
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
            '{' | '}' | '$' | '|' => break,
            x => {
                let _ = it.next();
                retval.push(x);
            }
        }
    }
    retval
}

fn read_placeholder_name(it: &mut Peekable<impl Iterator<Item = char>>) -> String {
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
            x if !x.is_alphabetic() && x != '_' => break,
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
    let mut escaped = false;
    for c in it {
        if escaped {
            escaped = false;
            retval.push(c);
            continue;
        }
        match c {
            '\\' => escaped = true,
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
