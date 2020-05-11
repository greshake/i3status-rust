use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use serde_derive::Deserialize;
use swayipc::reply::Event;
use swayipc::reply::{WindowChange, WorkspaceChange};
use swayipc::{Connection, EventType};
use uuid::Uuid;

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::de::deserialize_duration;
use crate::errors::*;
use crate::scheduler::Task;
use crate::widget::I3BarWidget;
use crate::widgets::text::TextWidget;

pub struct FocusedWindow {
    text: TextWidget,
    title: Arc<Mutex<String>>,
    marks: Arc<Mutex<String>>,
    show_marks: bool,
    max_width: usize,
    id: String,
    update_interval: Duration,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct FocusedWindowConfig {
    /// Truncates titles if longer than max-width
    #[serde(default = "FocusedWindowConfig::default_max_width")]
    pub max_width: usize,

    #[serde(
        default = "FocusedWindowConfig::default_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub interval: Duration,

    /// Show marks in place of title (if exist)
    #[serde(default = "FocusedWindowConfig::default_show_marks")]
    pub show_marks: bool,
}

impl FocusedWindowConfig {
    fn default_max_width() -> usize {
        21
    }

    fn default_interval() -> Duration {
        Duration::from_secs(30)
    }

    fn default_show_marks() -> bool {
        false
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
                                    let mut marks = marks_original.lock().unwrap();
                                    if !e.container.marks.is_empty() {
                                        *marks =
                                            e.container.marks.iter().map(|x| x.as_str()).collect();
                                    } else {
                                        *marks = String::from("");
                                    }
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
                                    if !e.container.marks.is_empty() {
                                        let mut marks = marks_original.lock().unwrap();
                                        *marks =
                                            e.container.marks.iter().map(|x| x.as_str()).collect();
                                        tx.send(Task {
                                            id: id_clone.clone(),
                                            update_time: Instant::now(),
                                        })
                                        .unwrap();
                                    }
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
            update_interval: block_config.interval,
        })
    }
}

impl Block for FocusedWindow {
    fn update(&mut self) -> Result<Option<Duration>> {
        //WIP: check if marks are non-empty first
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
        if self.show_marks && !marks_string.is_empty() {
            self.text.set_text(marks_string);
        } else {
            self.text.set_text(title_string);
        }

        Ok(Some(self.update_interval))
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
