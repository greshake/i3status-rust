extern crate nix;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Duration;
use std::io::BufReader;
use std::io::BufRead;
use std::fs::File;
use std::path::Path;
use serde_json::Value;
use block::{Block, MouseButton, Theme};

use self::nix::sys::statvfs::vfs::Statvfs;

const FORMAT: &'static str = "%a %F %T";

pub struct DiskUsage
{
    name: &'static str,
    target: &'static str,
    usage: RefCell<String>,
    statvfs: RefCell<Statvfs>,

}

impl DiskUsage
{
    pub fn new(name: &'static str, target: &'static str) -> DiskUsage
    {
        DiskUsage
            {
                name: name,
                target: target,
                usage: RefCell::new(String::from("unknown")),
                statvfs: RefCell::new(Statvfs::default()),

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

    fn get_status(&self, theme: &Theme) -> Value
    {
        let path = Path::new(self.target);
        let statvfs = &mut *self.statvfs.borrow_mut();

        statvfs.update_with_path(path);
        let free: f64 = (statvfs.f_bsize  * statvfs.f_bfree) as f64;
        let free_gb = free / (1024 * 1024 * 1024) as f64;


        *self.usage.borrow_mut() = format!("Free space on {0}: {1:.2} GB", self.target, free_gb);

        json!({
            "full_text" : self.usage.clone().into_inner(),
            "color"     : theme.fg.to_string()
        })
    }
}