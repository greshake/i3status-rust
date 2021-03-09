pub mod value;

use std::collections::HashMap;

use crate::errors::*;
use value::{Suffix, Unit, Value};

const MIN_WIDTH_TOKEN: char = ':';
const MAX_WIDTH_TOKEN: char = '^';
const MIN_SUFFIX_TOKEN: char = ';';
const UNIT_TOKEN: char = ';';
const BAR_MAX_VAL_TOKEN: char = '#';

#[derive(Debug, Clone)]
pub struct FormatTemplate {
    tokens: Vec<Token>,
}

#[derive(Debug, Clone)]
enum Token {
    Text(String),
    Var(Variable),
}

//TODO document
#[derive(Debug, Clone)]
pub struct Variable {
    pub name: String,
    pub min_width: Option<usize>,
    pub max_width: Option<usize>,
    pub pad_with: Option<char>,
    pub min_suffix: Option<Suffix>,
    pub unit: Option<Unit>,
    pub bar_max_value: Option<f64>,
}

impl FormatTemplate {
    pub fn from_string(s: &str) -> Result<Self> {
        let mut tokens = vec![];

        let mut text_buf = String::new();
        let mut var_buf = String::new();
        let mut min_width_buf = String::new();
        let mut max_width_buf = String::new();
        let mut min_suffix_buf = String::new();
        let mut unit_buf = String::new();
        let mut bar_max_value_buf = String::new();
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
                MIN_WIDTH_TOKEN if inside_var => {
                    if !min_width_buf.is_empty() {
                        //TODO return error
                    }
                    current_buf = &mut min_width_buf;
                }
                MAX_WIDTH_TOKEN if inside_var => {
                    if !min_width_buf.is_empty() {
                        //TODO return error
                    }
                    current_buf = &mut max_width_buf;
                }
                MIN_SUFFIX_TOKEN if inside_var => {
                    if !min_suffix_buf.is_empty() {
                        //TODO return error
                    }
                    current_buf = &mut min_suffix_buf;
                }
                UNIT_TOKEN if inside_var => {
                    if !unit_buf.is_empty() {
                        //TODO return error
                    }
                    current_buf = &mut unit_buf;
                }
                BAR_MAX_VAL_TOKEN if inside_var => {
                    if !bar_max_value_buf.is_empty() {
                        //TODO return error
                    }
                    current_buf = &mut bar_max_value_buf;
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
                    // Parse max_width
                    let max_width = if max_width_buf.is_empty() {
                        None
                    } else {
                        Some(max_width_buf.parse::<usize>().unwrap()) // Might return error
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
                    // Parse bar_max_value
                    let bar_max_value = if bar_max_value_buf.is_empty() {
                        None
                    } else {
                        Some(bar_max_value_buf.parse::<f64>().unwrap()) // Might return error
                    };
                    tokens.push(Token::Var(Variable {
                        name: var_buf.clone(),
                        min_width,
                        max_width,
                        pad_with,
                        min_suffix,
                        unit,
                        bar_max_value,
                    }));
                    // Clear all buffers
                    var_buf.clear();
                    min_width_buf.clear();
                    max_width_buf.clear();
                    min_suffix_buf.clear();
                    unit_buf.clear();
                    bar_max_value_buf.clear();
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
