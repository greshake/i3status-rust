use std::collections::HashMap;
use std::rc::Rc;

use crate::icons;
use crate::themes::Theme;

#[derive(Debug)]
pub struct Appearance {
    pub theme: Rc<Theme>,
    pub icons: Rc<HashMap<String, String>>,
}

impl Appearance {
    pub fn new(theme: Theme, icons: HashMap<String, String>) -> Self {
        Self {
            theme: Rc::new(theme),
            icons: Rc::new(icons),
        }
    }

    pub fn theme_override(&mut self, overrides: &HashMap<String, String>) {
        let mut theme = self.theme.as_ref().clone();
        for entry in overrides {
            match entry.0.as_str() {
                "idle_fg" => theme.idle_fg = Some(entry.1.to_string()),
                "idle_bg" => theme.idle_bg = Some(entry.1.to_string()),
                "info_fg" => theme.info_fg = Some(entry.1.to_string()),
                "info_bg" => theme.info_bg = Some(entry.1.to_string()),
                "good_fg" => theme.good_fg = Some(entry.1.to_string()),
                "good_bg" => theme.good_bg = Some(entry.1.to_string()),
                "warning_fg" => theme.warning_fg = Some(entry.1.to_string()),
                "warning_bg" => theme.warning_bg = Some(entry.1.to_string()),
                "critical_fg" => theme.critical_fg = Some(entry.1.to_string()),
                "critical_bg" => theme.critical_bg = Some(entry.1.to_string()),
                // TODO the below as well?
                // "separator"
                // "separator_bg"
                // "separator_fg"
                // "alternating_tint_bg"
                _ => (),
            }
        }
        self.theme = Rc::new(theme);
    }

    pub fn get_icon(&self, icon: &str) -> Option<String> {
        self.icons.get(icon).map(|s| s.to_string())
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Rc::new(Theme::default()),
            icons: Rc::new(icons::default()),
        }
    }
}

impl Clone for Appearance {
    fn clone(&self) -> Self {
        Appearance {
            theme: Rc::clone(&self.theme),
            icons: Rc::clone(&self.icons),
        }
    }
}
