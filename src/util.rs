use block::{Block, Theme};
use std::collections::HashMap;
use std::hash::Hash;

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

pub fn print_blocks(blocks: &Vec<&Block>, template: &HashMap<&str, String>, theme: &Theme) {
    print!("[");
    for (idx, block) in blocks.iter().enumerate() {
        print!("{{");

        // We get the status, and then we merge the template with it

        let mut first = true;
        let status = &block.get_status(theme);

        let mut merged: HashMap<&str, &str> = HashMap::new();

        for (key, value) in template.iter() {
            merged.insert(key, value.as_ref());
        }

        for (key, value) in status.iter() {
            merged.insert(key, &value);
        }

        if let Some(id) = block.id() {
            merged.insert("name", id);
        }

        for (name, value) in merged {
            if first {
                print!("\"{0}\": \"{1}\"", name, value);
                first = false;
            } else {
                print!(",\"{0}\": \"{1}\"", name, value);
            }

        }

        print!("}}");

        if idx != (blocks.len() - 1) {
            print!(",");
        }
    }
    println!("],");
}
