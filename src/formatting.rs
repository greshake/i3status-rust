pub mod placeholder;
pub mod prefix;
pub mod unit;
pub mod value;

use std::collections::HashMap;
use std::convert::TryInto;

use crate::config::SharedConfig;
use crate::errors::*;
use crate::widgets::{text::TextWidget, Spacing};
use placeholder::Placeholder;
use value::Value;

#[derive(Debug, Clone)]
pub struct FormatTemplate {
    tokens: Vec<Token>,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Text(String),
    Var(Placeholder),
}

#[derive(Debug, Clone)]
pub enum RenderedWidget {
    Text(TextWidget),
    Var(String, TextWidget),
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

    // Experimental function: avoid using this function in your block.
    // TODO reconsider the interface
    pub fn render_widgets(
        &self,
        config: SharedConfig,
        id: usize,
        vars: &HashMap<&str, Value>,
    ) -> Result<Vec<RenderedWidget>> {
        let mut rendered = Vec::new();

        for token in &self.tokens {
            match token {
                Token::Text(text) => rendered.push(RenderedWidget::Text(
                    TextWidget::new(id, rendered.len(), config.clone()).with_text(text),
                )),
                Token::Var(var) => rendered.push(RenderedWidget::Var(
                    var.name.clone(),
                    TextWidget::new(id, rendered.len(), config.clone())
                        .with_spacing(Spacing::Hidden)
                        .with_text(
                            &vars
                                .get(&*var.name)
                                .internal_error(
                                    "util",
                                    &format!("Unknown placeholder in format string: {}", var.name),
                                )?
                                .format(&var)?,
                        ),
                )),
            }
        }

        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prefix::Prefix;
    use unit::Unit;

    #[test]
    fn from_string() {
        let ft = FormatTemplate::from_string(
            "some text {var} var again {var*_}{new_var:3} {bar:2#100} {freq;1}.",
        );
        assert!(ft.is_ok());

        let mut tokens = ft.unwrap().tokens.into_iter();
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
        let ft = FormatTemplate::from_string(
            "some text {var} var again {var}{new_var:3} {bar:2#100} {freq;1}.",
        );
        assert!(ft.is_ok());

        let values = map!(
            "var" => Value::from_string("|var value|".to_string()),
            "new_var" => Value::from_integer(12),
            "bar" => Value::from_integer(25),
            "freq" => Value::from_float(0.01).hertz(),
        );

        assert_eq!(
            ft.unwrap().render(&values).unwrap().as_str(),
            "some text |var value| var again |var value| 12 \u{258c}  0.0Hz."
        );
    }
}
