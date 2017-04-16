use block::{Block};
use blocks::separator::Separator;
use serde_json::Value;
use serde_json::map::Map;

static SEP: Separator = Separator {};

macro_rules! map(
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

pub fn print_blocks(blocks: &Vec<&Block>, theme: &Value) {
    print!("[");
    let mut last_bg = Value::Null;
    for (idx, block) in blocks.iter().enumerate() {

        // We get the status, and then we merge the template with it
        let status = &block.get_status(theme);

        let (key_bg, key_fg) = block.get_state().theme_keys();

        let template = json!({
            "background": theme[key_bg],
            "color": theme[key_fg],
            "separator_block_width": 0,
            "separator": false
        });

        let merged = merge_json_obj(&template, status).unwrap();

        if let Value::Object(mut map) = merged {
            if let Some(id) = block.id() {
                map.insert(String::from("name"), Value::String(String::from(id)));
            }
            let m = Value::Object(map);
            let mut sep = merge_json_obj(&m, &SEP.get_status(theme)).unwrap();

            sep["color"] = m["background"].clone();
            sep["background"] = last_bg;
            last_bg = m["background"].clone();

            print!("{},", sep.to_string());
            print!("{}", m.to_string());
        }

        if idx != (blocks.len() - 1) {
            print!(",");
        }
    }
    println!("],");
}
