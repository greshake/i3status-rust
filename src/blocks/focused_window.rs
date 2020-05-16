use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use swayipc::reply::Event;
use swayipc::reply::{WindowChange, WorkspaceChange};
use swayipc::{Connection, EventType};
use uuid::Uuid;

use crate::blocks::Update;
use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::errors::*;
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarksType {
    All,
    Visible,
    None,
}

pub struct FocusedWindow {
    text: TextWidget,
    title: Arc<Mutex<String>>,
    marks: Arc<Mutex<String>>,
    show_marks: MarksType,
    max_width: usize,
    id: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct FocusedWindowConfig {
    /// Truncates titles if longer than max-width
    #[serde(default = "FocusedWindowConfig::default_max_width")]
    pub max_width: usize,

    /// Show marks in place of title (if exist)
    #[serde(default = "FocusedWindowConfig::default_show_marks")]
    pub show_marks: MarksType,
}

impl FocusedWindowConfig {
    fn default_max_width() -> usize {
        21
    }

    fn default_show_marks() -> MarksType {
        MarksType::None
    }
}

impl ConfigBlock for FocusedWindow {
    type Config = FocusedWindowConfig;

    fn new(block_config: Self::Config, config: Config, tx: Sender<Task>) -> Result<Self> {
        let id = Uuid::new_v4().to_simple().to_string();
        let id_clone = id.clone();

        let title_original = Arc::new(Mutex::new(String::from("")));
        let title = title_original.clone();
        let marks_original = Arc::new(Mutex::new(String::from("")));
        let marks = marks_original.clone();
        let marks_type = block_config.show_marks;

        let _test_conn =
            Connection::new().block_error("focused_window", "failed to acquire connect to IPC")?;

        thread::Builder::new()
            .name("focused_window".into())
            .spawn(move || {
                for event in Connection::new()
                    .unwrap()
                    .subscribe(&[EventType::Window, EventType::Workspace])
                    .unwrap()
                {
                    match event.unwrap() {
                        Event::Window(e) => {
                            match e.change {
                                WindowChange::Focus => {
                                    if let Some(name) = e.container.name {
                                        let mut title = title_original.lock().unwrap();
                                        *title = name;
                                    }

                                    let mut marks_str = String::from("");
                                    for mark in e.container.marks {
                                        match marks_type {
                                            MarksType::All => {
                                                marks_str.push_str(&format!("[{}]", mark));
                                            }
                                            MarksType::Visible => {
                                                if !mark.starts_with('_') {
                                                    marks_str.push_str(&format!("[{}]", mark));
                                                }
                                            }
                                            _ => (),
                                        }
                                    }
                                    let mut marks = marks_original.lock().unwrap();
                                    *marks = marks_str;

                                    tx.send(Task {
                                        id: id_clone.clone(),
                                        update_time: Instant::now(),
                                    })
                                    .unwrap();
                                }
                                WindowChange::Title => {
                                    if e.container.focused {
                                        if let Some(name) = e.container.name {
                                            let mut title = title_original.lock().unwrap();
                                            *title = name;
                                            tx.send(Task {
                                                id: id_clone.clone(),
                                                update_time: Instant::now(),
                                            })
                                            .unwrap();
                                        }
                                    }
                                }
                                WindowChange::Mark => {
                                    let mut marks_str = String::from("");
                                    for mark in e.container.marks {
                                        match marks_type {
                                            MarksType::All => {
                                                marks_str.push_str(&format!("[{}]", mark));
                                            }
                                            MarksType::Visible => {
                                                if !mark.starts_with('_') {
                                                    marks_str.push_str(&format!("[{}]", mark));
                                                }
                                            }
                                            _ => (),
                                        }
                                    }
                                    let mut marks = marks_original.lock().unwrap();
                                    *marks = marks_str;

                                    tx.send(Task {
                                        id: id_clone.clone(),
                                        update_time: Instant::now(),
                                    })
                                    .unwrap();
                                }
                                WindowChange::Close => {
                                    if let Some(name) = e.container.name {
                                        let mut title = title_original.lock().unwrap();
                                        if name == *title {
                                            *title = String::from("");
                                            tx.send(Task {
                                                id: id_clone.clone(),
                                                update_time: Instant::now(),
                                            })
                                            .unwrap();
                                        }
                                    }
                                }
                                _ => {}
                            };
                        }
                        Event::Workspace(e) => {
                            if let WorkspaceChange::Init = e.change {
                                let mut title = title_original.lock().unwrap();
                                *title = String::from("");
                                tx.send(Task {
                                    id: id_clone.clone(),
                                    update_time: Instant::now(),
                                })
                                .unwrap();
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            })
            .unwrap();

        Ok(FocusedWindow {
            id,
            text: TextWidget::new(config),
            max_width: block_config.max_width,
            show_marks: block_config.show_marks,
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
            MarksType::None => title_string,
            _ => {
                if !marks_string.is_empty() {
                    marks_string
                } else {
                    title_string
                }
            }
        };
        self.text.set_text(out_str);

        Ok(None)
    }

    fn view(&self) -> Vec<&dyn I3BarWidget> {
        let title = &*self.title.lock().unwrap();
        if String::is_empty(title) {
            vec![]
        } else {
            vec![&self.text]
        }
    }

    fn id(&self) -> &str {
        &self.id
    }
}
