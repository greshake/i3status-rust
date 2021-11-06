use crate::themes::Color;

/// Represent block as described in https://i3wm.org/docs/i3bar-protocol.html

#[derive(Debug, Clone)]
pub struct I3BarBlock {
    pub full_text: String,
    pub short_text: Option<String>,
    pub color: Color,
    pub background: Color,
    pub border: Option<String>,
    pub border_top: Option<usize>,
    pub border_right: Option<usize>,
    pub border_bottom: Option<usize>,
    pub border_left: Option<usize>,
    pub min_width: Option<I3BarBlockMinWidth>,
    pub align: Option<I3BarBlockAlign>,
    pub name: Option<String>,
    pub instance: Option<String>,
    pub urgent: Option<bool>,
    pub separator: Option<bool>,
    pub separator_block_width: Option<usize>,
    pub markup: Option<String>,
}

macro_rules! json_add_str {
    ($retval:ident, $obj:expr, $name:expr) => {
        if let Some(ref val) = $obj {
            $retval.push_str(&format!(
                "\"{}\":\"{}\",",
                stringify!($name),
                val.chars()
                    .map(|c| match c {
                        '\\' => "\\\\".to_string(),
                        '\n' => "\\n".to_string(),
                        '"' => "\\\"".to_string(),
                        x => x.to_string(),
                    })
                    .collect::<String>(),
            ));
        }
    };
}
macro_rules! json_add_val {
    ($retval:ident, $obj:expr, $name:expr) => {
        if let Some(val) = $obj {
            $retval.push_str(&format!("\"{}\":{},", stringify!($name), val));
        }
    };
}

impl I3BarBlock {
    pub fn render(&self) -> String {
        let mut retval = String::from("{");

        json_add_str!(retval, Some(&self.full_text), full_text);
        json_add_str!(retval, self.short_text, short_text);
        json_add_str!(retval, self.color.to_string(), color);
        json_add_str!(retval, self.background.to_string(), background);
        json_add_str!(retval, self.border, border);
        json_add_val!(retval, self.border_top, border_top);
        json_add_val!(retval, self.border_right, border_right);
        json_add_val!(retval, self.border_bottom, border_bottom);
        json_add_val!(retval, self.border_left, border_left);
        match self.min_width {
            Some(I3BarBlockMinWidth::Pixels(x)) => json_add_val!(retval, Some(x), min_width),
            Some(I3BarBlockMinWidth::Text(ref x)) => json_add_str!(retval, Some(x), min_width),
            None => {}
        }
        match self.align {
            Some(I3BarBlockAlign::Center) => retval.push_str("\"align\":\"center\","),
            Some(I3BarBlockAlign::Right) => retval.push_str("\"align\":\"right\","),
            Some(I3BarBlockAlign::Left) => retval.push_str("\"align\":\"left\","),
            None => {}
        }
        json_add_str!(retval, self.name, name);
        json_add_str!(retval, self.instance, instance);
        json_add_val!(retval, self.urgent, urgent);
        json_add_val!(retval, self.separator, separator);
        json_add_val!(retval, self.separator_block_width, separator_block_width);
        json_add_str!(retval, self.markup, markup);

        retval.pop();
        retval.push('}');
        retval
    }
}

impl Default for I3BarBlock {
    fn default() -> Self {
        #[cfg(not(feature = "debug_borders"))]
        let border = None;
        #[cfg(feature = "debug_borders")]
        let border = Some("#ff0000".to_string());
        Self {
            full_text: String::new(),
            short_text: None,
            color: Color::None,
            background: Color::None,
            border,
            border_top: None,
            border_right: None,
            border_bottom: None,
            border_left: None,
            min_width: None,
            align: None,
            name: None,
            instance: None,
            urgent: None,
            separator: Some(false),
            separator_block_width: Some(0),
            markup: Some("pango".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum I3BarBlockAlign {
    Center,
    Right,
    Left,
}

#[derive(Debug, Clone)]
pub enum I3BarBlockMinWidth {
    Pixels(usize),
    Text(String),
}
