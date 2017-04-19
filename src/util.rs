use block::Block;
use serde_json::Value;
use serde_json::map::Map;
use widget::*;

macro_rules! map (
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = ::std::collections::HashMap::new();
            $(
                m.insert($key, $value);
            )+
            m
        }
     };
);

pub fn print_blocks(blocks: &Vec<&Block>) {
    let mut elements: Vec<Box<UIElement>> = Vec::new();
    for block in blocks {
        elements.push(block.get_ui());
    }
    let bar = UIElement::Block(elements);
    bar.print_elements();
}

macro_rules! get_str{
    ($config:expr, $name:expr) => {String::from($config[$name].as_str().expect(&format!("Required argument {} not found in block config!", $name)))};
}
macro_rules! get_str_default {
    ($config:expr, $name:expr, $default:expr) => {String::from($config[$name].as_str().unwrap_or($default))};
}

macro_rules! get_u64 {
    ($config:expr, $name:expr) => {$config[$name].as_u64().expect(&format!("Required argument {} not found in block config!", $name))};
}
macro_rules! get_u64_default {
    ($config:expr, $name:expr, $default:expr) => {$config[$name].as_u64().unwrap_or($default)};
}

// UI- single widget from Widget field
macro_rules! ui ( {$widget_field: expr} => { Box::new(UIElement::WidgetWithSeparator(Box::new($widget_field.clone().into_inner()) as Box<Widget>)) }; );

macro_rules! ui_list ( {$($widget_field: expr), +} => {
        {
            let mut elements: Vec<Box<UIElement>> = Vec::new();
            $(
                elements.push(Box::new(UIElement::WidgetWithSeparator(Box::new($widget_field.clone().into_inner()) as Box<Widget>)));
            )+
            Box::new(UIElement::Block(elements))
        }

    };
);