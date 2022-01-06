use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use swayipc::{Connection, Event, EventType, Node, WindowChange, WorkspaceChange};

use crate::blocks::{Block, ConfigBlock, Update};
use crate::config::SharedConfig;
use crate::errors::*;
use crate::formatting::value::Value;
use crate::formatting::FormatTemplate;
use crate::scheduler::Task;
use crate::util::escape_pango_text;
use crate::widgets::text::TextWidget;
use crate::widgets::I3BarWidget;

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarksType {
    All,
    Visible,
    None,
}

pub struct FocusedWindow {
    id: usize,
    text: TextWidget,
    title: Arc<Mutex<String>>,
    marks: Arc<Mutex<String>>,
    show_marks: MarksType,
    format: FormatTemplate,
    max_width: usize,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct FocusedWindowConfig {
    /// Truncates titles if longer than max-width
    pub max_width: usize,

    /// Show marks in place of title (if exist)
    pub show_marks: MarksType,

    /// Format override
    pub format: FormatTemplate,
}

impl Default for FocusedWindowConfig {
    fn default() -> Self {
        Self {
            max_width: 21,
            show_marks: MarksType::None,
            format: FormatTemplate::default(),
        }
    }
}

impl ConfigBlock for FocusedWindow {
    type Config = FocusedWindowConfig;

    fn new(
        id: usize,
        block_config: Self::Config,
        shared_config: SharedConfig,
        tx: Sender<Task>,
    ) -> Result<Self> {
        let title = Arc::new(Mutex::new(String::from("")));
        let marks = Arc::new(Mutex::new(String::from("")));
        let marks_type = block_config.show_marks;

        let update_window = {
            let title = title.clone();

            move |new_title| {
                let mut title = title
                    .lock()
                    .expect("lock has been poisoned in `window` block");

                let changed = *title != new_title;
                *title = new_title;
                changed
            }
        };

        let close_window = {
            let title = title.clone();

            move |closed_title: String| {
                let mut title = title
                    .lock()
                    .expect("lock has been poisoned in `window` block");

                if *title == closed_title {
                    *title = "".to_string();
                    true
                } else {
                    false
                }
            }
        };

        let update_marks = {
            let marks = marks.clone();

            move |new_marks: Vec<String>| {
                let mut new_marks_str = String::from("");

                for mark in new_marks {
                    match marks_type {
                        MarksType::All => {
                            new_marks_str.push_str(&format!("[{}]", mark));
                        }
                        MarksType::Visible if !mark.starts_with('_') => {
                            new_marks_str.push_str(&format!("[{}]", mark));
                        }
                        _ => {}
                    }
                }

                let mut marks = marks
                    .lock()
                    .expect("lock has been poisoned in `window` block");

                let changed = *marks != new_marks_str;
                *marks = new_marks_str;
                changed
            }
        };

        let _test_conn =
            Connection::new().block_error("focused_window", "failed to acquire connect to IPC")?;

        thread::Builder::new()
            .name("focused_window".into())
            .spawn(move || {
                let conn = Connection::new().expect("failed to open connection with swayipc");

                let events = conn
                    .subscribe(&[EventType::Window, EventType::Workspace])
                    .expect("could not subscribe to window events");

                for event in events {
                    let updated = match event.expect("could not read event in `window` block") {
                        Event::Window(e) => match (e.change, e.container) {
                            (WindowChange::Mark, Node { marks, .. }) => update_marks(marks),
                            (WindowChange::Focus, Node { name, marks, .. }) => {
                                let updated_for_window = name.map(&update_window).unwrap_or(false);
                                let updated_for_marks = update_marks(marks);
                                updated_for_window || updated_for_marks
                            }
                            (
                                WindowChange::Title,
                                Node {
                                    focused: true,
                                    name: Some(name),
                                    ..
                                },
                            ) => update_window(name),
                            (
                                WindowChange::Close,
                                Node {
                                    name: Some(name), ..
                                },
                            ) => close_window(name),
                            _ => false,
                        },
                        Event::Workspace(e) if e.change == WorkspaceChange::Init => {
                            update_window("".to_string())
                        }
                        _ => false,
                    };

                    if updated {
                        tx.send(Task {
                            id,
                            update_time: Instant::now(),
                        })
                        .expect("could not communicate with channel in `window` block");
                    }
                }
            })
            .expect("failed to start watching thread for `window` block");

        let text = TextWidget::new(id, 0, shared_config);
        Ok(FocusedWindow {
            id,
            text,
            max_width: block_config.max_width,
            show_marks: block_config.show_marks,
            format: block_config.format.with_default("{combo}")?,
            title,
            marks,
        })
    }
}

impl Block for FocusedWindow {
    fn update(&mut self) -> Result<Option<Update>> {
        let mut marks_string = (*self
            .marks
            .lock()
            .block_error("focused_window", "failed to acquire lock")?)
        .clone();
        marks_string = marks_string.chars().take(self.max_width).collect();
        let mut title_string = (*self
            .title
            .lock()
            .block_error("focused_window", "failed to acquire lock")?)
        .clone();
        title_string = title_string.chars().take(self.max_width).collect();
        let out_str = match self.show_marks {
            MarksType::None => &title_string,
            _ => {
                if !marks_string.is_empty() {
                    &marks_string
                } else {
                    &title_string
                }
            }
        };
        let values = map!(
            "combo" => Value::from_string(escape_pango_text(out_str)),
            "marks" => Value::from_string(escape_pango_text(&marks_string)),
            "title" => Value::from_string(escape_pango_text(&title_string))
        );

        self.text.set_texts(self.format.render(&values)?);

        Ok(None)
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        let title = &*self
            .title
            .lock()
            .expect("lock has been poisoned in `window` block");

        if title.is_empty() {
            vec![]
        } else {
            vec![&self.text]
        }
    }

    fn id(&self) -> usize {
        self.id
    }
}
