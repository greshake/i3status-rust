use block::{Block, State};
use serde_json::Value;
use serde_json::map::Map;

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

fn merge_json_obj(v1: &Value, v2: &Value) -> Option<Value> {
    use Value::Object;
    if let &Object(ref map1) = v1 {
        if let &Object(ref map2) = v2 {
            let mut map_merged = Map::new();

            for (k, v) in map1 {
                map_merged.insert(k.clone(), v.clone());
            }

            for (k, v) in map2 {
                map_merged.insert(k.clone(), v.clone());
            }

            return Some(Object(map_merged));
        }
    }
    None
}

pub fn render(status: Value, opt_id: Option<&str>, state: State, theme: &Value) -> Value {
    let (key_bg, key_fg) = state.theme_keys();

    let template = json!({
            "background": theme[key_bg],
            "color": theme[key_fg],
            "separator_block_width": 0,
            "separator": false
        });

    let mut block = merge_json_obj(&template, &status).unwrap();

    if let Some(id) = opt_id {
        block["name"] = Value::String(String::from(id));
    }

    block
}

pub fn print_blocks(blocks: &Vec<&Block>, theme: &Value) {
    print!("[");
    let mut last_bg = Value::Null;
    for (idx, block) in blocks.iter().enumerate() {
        let blo = render(block.get_status(theme),
                         block.id(),
                         block.get_state(),
                         theme);

        let sep = json!({
            "full_text": "".to_string(),
            "separator": false,
            "separator_block_width": 0,
            "background": last_bg,
            "color": blo["background"].clone()
        });

        last_bg = blo["background"].clone();

        print!("{},", sep.to_string());
        print!("{}", blo.to_string());

        if idx != (blocks.len() - 1) {
            print!(",");
        }
    }
    println!("],");
}
