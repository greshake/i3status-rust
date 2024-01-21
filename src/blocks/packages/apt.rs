use std::env;
use std::process::Stdio;

use tokio::fs::{create_dir_all, File};
use tokio::process::Command;

use super::*;

#[derive(Default)]
pub struct Apt {
    pub(super) config_file: String,
    pub(super) ignore_phased_updates: bool,
}

impl Apt {
    pub async fn new(ignore_phased_updates: bool) -> Result<Self> {
        let mut apt = Apt {
            config_file: String::new(),
            ignore_phased_updates,
        };

        apt.setup().await?;

        Ok(apt)
    }

    async fn is_phased_update(&self, package_line: &str) -> Result<bool> {
        let package_name_regex = regex!(r#"(.*)/.*"#);
        let package_name = &package_name_regex
            .captures(package_line)
            .error("Couldn't find package name")?[1];

        let output = String::from_utf8(
            Command::new("apt-cache")
                .args(["-c", &self.config_file, "policy", package_name])
                .output()
                .await
                .error("Problem running apt-cache command")?
                .stdout,
        )
        .error("Problem capturing apt-cache command output")?;

        let phased_regex = regex!(r".*\(phased (\d+)%\).*");
        Ok(match phased_regex.captures(&output) {
            Some(matches) => &matches[1] != "100",
            None => false,
        })
    }

    async fn setup(&mut self) -> Result<()> {
        let mut cache_dir = env::temp_dir();
        cache_dir.push("i3rs-apt");
        if !cache_dir.exists() {
            create_dir_all(&cache_dir)
                .await
                .error("Failed to create temp dir")?;
        }

        let apt_config = format!(
            "Dir::State \"{}\";\n
         Dir::State::lists \"lists\";\n
         Dir::Cache \"{}\";\n
         Dir::Cache::srcpkgcache \"srcpkgcache.bin\";\n
         Dir::Cache::pkgcache \"pkgcache.bin\";",
            cache_dir.display(),
            cache_dir.display(),
        );

        let mut config_file = cache_dir;
        config_file.push("apt.conf");
        let config_file = config_file.to_str().unwrap();

        self.config_file = config_file.to_string();

        let mut file = File::create(&config_file)
            .await
            .error("Failed to create config file")?;
        file.write_all(apt_config.as_bytes())
            .await
            .error("Failed to write to config file")?;

        Ok(())
    }
}

#[async_trait]
impl Backend for Apt {
    fn name(&self) -> &str {
        "apt"
    }

    async fn get_updates_list(&self) -> Result<Vec<String>> {
        Command::new("apt")
            .env("APT_CONFIG", &self.config_file)
            .args(["update"])
            .stdout(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .error("Failed to run `apt update`")?
            .wait()
            .await
            .error("Failed to run `apt update`")?;
        let stdout = Command::new("apt")
            .env("LANG", "C")
            .env("APT_CONFIG", &self.config_file)
            .args(["list", "--upgradable"])
            .output()
            .await
            .error("Problem running apt command")?
            .stdout;

        let updates = String::from_utf8(stdout).error("apt produced non-UTF8 output")?;
        let updates: Vec<String> = updates
            .lines()
            .filter(|line| line.contains("[upgradable"))
            .filter_map(|update_line| {
                let is_phased_update =
                    async { self.is_phased_update(update_line).await.unwrap_or(false) };

                Some(update_line.to_string()).filter(|_| {
                    !self.ignore_phased_updates
                        || !tokio::task::block_in_place(|| {
                            tokio::runtime::Runtime::new()
                                .unwrap()
                                .block_on(is_phased_update)
                        })
                })
            })
            .collect();

        Ok(updates)
    }
}
