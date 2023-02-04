use crate::themes::color::Color;
use serde::Serialize;

/// Represent block as described in <https://i3wm.org/docs/i3bar-protocol.html>
#[derive(Serialize, Debug, Clone)]
pub struct I3BarBlock {
    pub full_text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub short_text: String,
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
    /// This project uses `name` field to uniquely identify each "logical block". For example two
    /// "config blocks" merged using `merge_with_next` will have the same `name`. This information
    /// could be used by some bar frontends (such as `i3bar-river`) and will be ignored by `i3bar`
    /// and `swaybar`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// This project uses `instance` field to uniquely identify each block and optionally a part
    /// of the block, e.g. a "button". The format is `{block_id}:{optional_widget_name}`. This info
    /// is used when dispatching click events.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub instance: String,
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
            short_text: String::new(),
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
            instance: String::new(),
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
