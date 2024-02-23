use debounced::{debounced, Debounced};
use inotify::{EventStream, Inotify, WatchDescriptor, WatchMask, Watches};
use tokio::fs::{read_dir, File};
use tokio::time::{interval, Interval};

use std::path::PathBuf;

use super::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(rename_all = "lowercase", deny_unknown_fields, default)]
pub struct Config {
    exclude_device: Vec<PathBuf>,
    #[default(vec!["pipewire".into(), "wireplumber".into()])]
    exclude_consumer: Vec<String>,
}

pub(super) struct Monitor<'a> {
    config: &'a Config,
    devices: HashMap<PathBuf, WatchDescriptor>,
    interval: Interval,
    watches: Watches,
    updates: Debounced<EventStream<[u8; 1024]>>,
}

impl<'a> Monitor<'a> {
    pub(super) async fn new(config: &'a Config, duration: Duration) -> Result<Self> {
        let notify = Inotify::init().error("Failed to start inotify")?;
        let watches = notify.watches();

        let updates = debounced(
            notify
                .into_event_stream([0; 1024])
                .error("Failed to create event stream")?,
            Duration::from_millis(100),
        );

        let mut s = Self {
            config,
            devices: HashMap::new(),
            interval: interval(duration),
            watches,
            updates,
        };
        s.update_devices().await?;

        Ok(s)
    }

    async fn update_devices(&mut self) -> Result<bool> {
        let mut changes = false;
        let mut devices_to_remove: HashMap<PathBuf, WatchDescriptor> = self.devices.clone();
        let mut sysfs_paths = read_dir("/dev").await.error("Unable to read /dev")?;
        while let Some(entry) = sysfs_paths
            .next_entry()
            .await
            .error("Unable to get next device in /dev")?
        {
            if let Some(file_name) = entry.file_name().to_str() {
                if !file_name.starts_with("video") {
                    continue;
                }
            }

            let sysfs_path = entry.path();

            if self.config.exclude_device.contains(&sysfs_path) {
                debug!("ignoring {:?}", sysfs_path);
                continue;
            }

            if self.devices.contains_key(&sysfs_path) {
                devices_to_remove.remove(&sysfs_path);
            } else {
                debug!("adding watch {:?}", sysfs_path);
                self.devices.insert(
                    sysfs_path.clone(),
                    self.watches
                        .add(&sysfs_path, WatchMask::OPEN | WatchMask::CLOSE)
                        .error("Failed to watch data location")?,
                );
                changes = true;
            }
        }
        for (sysfs_path, wd) in devices_to_remove {
            debug!("removing watch {:?}", sysfs_path);
            self.devices.remove(&sysfs_path);
            self.watches
                .remove(wd)
                .error("Failed to unwatch data location")?;
            changes = true;
        }

        Ok(changes)
    }
}

#[async_trait]
impl<'a> PrivacyMonitor for Monitor<'a> {
    async fn get_info(&mut self) -> Result<PrivacyInfo> {
        let mut mapping: PrivacyInfo = PrivacyInfo::new();

        let mut proc_paths = read_dir("/proc").await.error("Unable to read /proc")?;
        while let Some(proc_path) = proc_paths
            .next_entry()
            .await
            .error("Unable to get next device in /proc")?
        {
            let proc_path = proc_path.path();
            let fd_path = proc_path.join("fd");
            let Ok(mut fd_paths) = read_dir(fd_path).await else {
                continue;
            };
            while let Ok(Some(fd_path)) = fd_paths.next_entry().await {
                let Ok(link_path) = fd_path.path().read_link() else {
                    continue;
                };
                if self.devices.contains_key(&link_path) {
                    let Ok(mut file) = File::open(proc_path.join("comm")).await else {
                        continue;
                    };
                    let mut contents = String::new();
                    if file.read_to_string(&mut contents).await.is_ok() {
                        let reader = contents.trim_end().to_string();
                        if self.config.exclude_consumer.contains(&reader) {
                            continue;
                        }
                        debug!("{} {:?}", reader, link_path);
                        *mapping
                            .entry(Type::Webcam)
                            .or_default()
                            .entry(link_path.to_string_lossy().to_string())
                            .or_default()
                            .entry(reader)
                            .or_default() += 1;
                        debug!("{:?}", mapping);
                    }
                }
            }
        }
        Ok(mapping)
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        loop {
            select! {
                _ = self.interval.tick() => {
                    if self.update_devices().await? {
                        break;
                    }
                },
                _ = self.updates.next() => break,
            }
        }
        Ok(())
    }
}
