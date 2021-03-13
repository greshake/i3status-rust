pub mod placeholder;
pub mod prefix;
pub mod unit;
pub mod value;

use std::collections::HashMap;
use std::convert::TryInto;

use crate::errors::*;
use placeholder::Placeholder;
use value::Value;

#[derive(Debug, Clone)]
pub struct FormatTemplate {
    tokens: Vec<Token>,
}

#[derive(Debug, Clone)]
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

impl FormatTemplate {
    pub fn from_string(s: &str) -> Result<Self> {
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
                        .format(&var)?,
                ),
            }
        }

        Ok(rendered)
    }
}
