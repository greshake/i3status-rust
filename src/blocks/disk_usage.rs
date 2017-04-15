extern crate nix;

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;
use std::io::BufReader;
use std::io::BufRead;
use std::fs::File;
use std::path::Path;

use block::{Block, MouseButton, Theme};

use self::nix::sys::statvfs::vfs::Statvfs;

const FORMAT: &'static str = "%a %F %T";

pub struct DiskUsage
{
    name: &'static str,
    target: &'static str,
    usage: RefCell<String>,

}

impl DiskUsage
{
    pub fn new(name: &'static str, target: &'static str) -> DiskUsage
    {
        DiskUsage
            {
                name: name,
                target: target,
                usage: RefCell::new(String::from("Hello World")),

            }
    }
}

impl Block for DiskUsage
{
    fn id(&self) -> Option<&str> {
        Some(self.name)
    }

    fn update(&self) -> Option<Duration>
    {
        Some(Duration::new(5, 0))
    }

    fn get_status(&self, theme: &Theme) -> HashMap<&str, String>
    {
        let path = Path::new(self.target);

        map! {
            "full_text" => self.usage.clone().into_inner(),
            "color"     => theme.bg.to_string()
        }
    }
}