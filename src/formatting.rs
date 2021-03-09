pub mod value;

use std::collections::HashMap;

use crate::errors::*;
use value::{Suffix, Unit, Value};

#[derive(Debug, Clone)]
pub struct FormatTemplate {
    tokens: Vec<Token>,
}

#[derive(Debug, Clone)]
enum Token {
    Text(String),
    Var(Variable),
}

#[derive(Debug, Clone)]
pub struct Variable {
    pub name: String,
    pub min_width: Option<usize>,
    pub pad_with: Option<char>,
    pub min_suffix: Option<Suffix>,
    pub unit: Option<Unit>,
}

#[derive(Debug, Clone)]
pub struct Padding {
    pub min_width: usize,
    pub pad_with: char,
}

impl FormatTemplate {
    pub fn from_string(s: &str) -> Result<Self> {
        let mut tokens = vec![];

        let mut text_buf = String::new();
        let mut var_buf = String::new();
        let mut min_width_buf = String::new();
        let mut min_suffix_buf = String::new();
        let mut unit_buf = String::new();
        let mut inside_var = false;

        let mut current_buf = &mut text_buf;

        for c in s.chars() {
            //TODO allow icon overrideing: `{var~icon}`
            match c {
                '{' => {
                    if inside_var {
                        // TODO return error
                    }
                    if !text_buf.is_empty() {
                        tokens.push(Token::Text(text_buf.clone()));
                        text_buf.clear();
                    }
                    current_buf = &mut var_buf;
                    inside_var = true;
                }
                ':' if inside_var => {
                    if !min_width_buf.is_empty() {
                        //TODO return error
                    }
                    current_buf = &mut min_width_buf;
                }
                ';' if inside_var => {
                    if !min_suffix_buf.is_empty() {
                        //TODO return error
                    }
                    current_buf = &mut min_suffix_buf;
                }
                '*' if inside_var => {
                    if !unit_buf.is_empty() {
                        //TODO return error
                    }
                    current_buf = &mut unit_buf;
                }
                '}' => {
                    if !inside_var {
                        //TODO return error
                    }
                    if var_buf.is_empty() {
                        //TODO return error
                    }
                    // Parse padding
                    let (min_width, pad_with) = if min_width_buf.is_empty() {
                        (None, None)
                    } else {
                        if min_width_buf.chars().next().unwrap() == '0' {
                            (
                                Some(min_width_buf[1..].parse().unwrap()), // Might return error
                                Some('0'),
                            )
                        } else {
                            (
                                Some(min_width_buf.parse().unwrap()), // Might return error
                                None,
                            )
                        }
                    };
                    // Parse min_suffix
                    let min_suffix = if min_suffix_buf.is_empty() {
                        None
                    } else {
                        Some(Suffix::from_string(&min_suffix_buf)?)
                    };
                    // Parse unit
                    let unit = if unit_buf.is_empty() {
                        None
                    } else {
                        Some(Unit::from_string(&unit_buf))
                    };
                    tokens.push(Token::Var(Variable {
                        name: var_buf.clone(),
                        min_width,
                        pad_with,
                        min_suffix,
                        unit,
                    }));
                    // Clear all buffers
                    var_buf.clear();
                    min_width_buf.clear();
                    min_suffix_buf.clear();
                    unit_buf.clear();
                    current_buf = &mut text_buf;
                    inside_var = false;
                }
                x => current_buf.push(x),
            }
        }
        if inside_var {
            //TODO return error
        }
        if !text_buf.is_empty() {
            tokens.push(Token::Text(text_buf.clone()));
        }

        //dbg!(&tokens);

        Ok(FormatTemplate { tokens })
    }

    pub fn render(&self, vars: &HashMap<&str, Value>) -> Result<String> {
        let mut rendered = String::new();

        for token in &self.tokens {
            match token {
                Token::Text(text) => rendered.push_str(&text),
                Token::Var(var) => rendered.push_str(
                    &vars
                        .get(&*var.name)
                        .internal_error(
                            "util",
                            &format!("Unknown placeholder in format string: {}", var.name),
                        )?
                        .format(&var),
                ),
            }
        }

        Ok(rendered)
    }
}
