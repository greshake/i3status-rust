// use crate::escape::JsonStr;
use crate::themes::Color;
use serde_derive::Serialize;

/// Represent block as described in <https://i3wm.org/docs/i3bar-protocol.html>
#[derive(Serialize, Debug, Clone)]
pub struct I3BarBlock {
    pub full_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_text: Option<String>,
    #[serde(skip_serializing_if = "Color::skip_ser")]
    pub color: Color,
    #[serde(skip_serializing_if = "Color::skip_ser")]
    pub background: Color,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_top: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_right: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_bottom: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_left: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_width: Option<I3BarBlockMinWidth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<I3BarBlockAlign>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urgent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator_block_width: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markup: Option<String>,
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

#[derive(Serialize, Debug, Clone, Copy)]
#[allow(dead_code)]
#[serde(rename_all = "lowercase")]
pub enum I3BarBlockAlign {
    Center,
    Right,
    Left,
}

#[derive(Serialize, Debug, Clone)]
#[allow(dead_code)]
#[serde(untagged)]
pub enum I3BarBlockMinWidth {
    Pixels(usize),
    Text(String),
}
