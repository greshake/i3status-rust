use std::time::{Duration, Instant};
use crossbeam_channel::Sender;
use std::thread;
use std::sync::{Arc, Mutex};

use crate::blocks::{Block, ConfigBlock};
use crate::config::Config;
use crate::errors::*;
use crate::widgets::text::TextWidget;
use crate::widget::I3BarWidget;
use crate::scheduler::Task;

use uuid::Uuid;

extern crate i3ipc;
use self::i3ipc::I3EventListener;
use self::i3ipc::Subscription;
use self::i3ipc::event::Event;
use self::i3ipc::event::inner::{WindowChange, WorkspaceChange};

pub struct FocusedWindow {
    text: TextWidget,
    title: Arc<Mutex<String>>,
    max_width: usize,
    id: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct FocusedWindowConfig {
    /// Truncates titles if longer than max-width
    #[serde(default = "FocusedWindowConfig::default_max_width")]
    pub max_width: usize,
}

impl FocusedWindowConfig {
    fn default_max_width() -> usize {
        21
    }
}

impl ConfigBlock for FocusedWindow {
    type Config = FocusedWindowConfig;

    fn new(block_config: Self::Config, config: Config, tx: Sender<Task>) -> Result<Self> {
        let id = Uuid::new_v4().simple().to_string();
        let id_clone = id.clone();

        let title_original = Arc::new(Mutex::new(String::from("")));
        let title = title_original.clone();

        thread::spawn(move || {
            // establish connection.
            let mut listener = I3EventListener::connect().unwrap();

            // subscribe to a couple events.
            let subs = [Subscription::Window, Subscription::Workspace];
            listener.subscribe(&subs).unwrap();

            // handle them
            for event in listener.listen() {
                match event.unwrap() {
                    Event::WindowEvent(e) => {
                        match e.change {
                            WindowChange::Focus => if let Some(name) = e.container.name {
                                let mut title = title_original.lock().unwrap();
                                *title = name;
                                tx.send(Task {
                                    id: id_clone.clone(),
                                    update_time: Instant::now(),
                                }).unwrap();
                            },
                            WindowChange::Title => if e.container.focused {
                                if let Some(name) = e.container.name {
                                    let mut title = title_original.lock().unwrap();
                                    *title = name;
                                    tx.send(Task {
                                        id: id_clone.clone(),
                                        update_time: Instant::now(),
                                    }).unwrap();
                                }
                            },
                            WindowChange::Close => if let Some(name) = e.container.name {
                                let mut title = title_original.lock().unwrap();
                                if name == *title {
                                    *title = String::from("");
                                    tx.send(Task {
                                        id: id_clone.clone(),
                                        update_time: Instant::now(),
                                    }).unwrap();
                                }
                            },
                            _ => {}
                        };
                    }
                    Event::WorkspaceEvent(e) => {
                        if let WorkspaceChange::Init = e.change {
                            let mut title = title_original.lock().unwrap();
                            *title = String::from("");
                            tx.send(Task {
                                id: id_clone.clone(),
                                update_time: Instant::now(),
                            }).unwrap();
                        }
                    },
                    _ => unreachable!(),
                }
            }
        });

        Ok(FocusedWindow {
            id,
            text: TextWidget::new(config),
            max_width: block_config.max_width,
            title,
        })
    }
}


impl Block for FocusedWindow {
    fn update(&mut self) -> Result<Option<Duration>> {
        let mut string = (*self.title
            .lock()
            .block_error("focused_window", "failed to acquire lock")?)
            .clone();
        string = string.chars().take(self.max_width).collect();
        self.text.set_text(string);
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
