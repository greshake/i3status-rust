//! The number of GitHub notifications
//!
//! This block shows the unread notification count for a GitHub account. A GitHub [personal access token](https://github.com/settings/tokens/new) with the "notifications" scope is required, and must be passed using the `I3RS_GITHUB_TOKEN` environment variable or `token` configuration option. Optionally the colour of the block is determined by the highest notification in the following lists from highest to lowest: `critical`,`warning`,`info`,`good`
//!
//! # Configuration
//!
//! Key | Values | Required | Default
//! ----|--------|----------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | No | `"$total.eng(1)"`
//! `interval` | Update interval in seconds | No | `30`
//! `token` | A GitHub personal access token with the "notifications" scope | No | None
//! `hide_if_total_is_zero` | Hide this block if the total count of notifications is zero | No | `false`
//! `critical` | List of notification types that change the block to the critical colour | No | None
//! `warning` | List of notification types that change the block to the warning colour | No | None
//! `info` | List of notification types that change the block to the info colour | No | None
//! `good` | List of notification types that change the block to the good colour | No | None
//!
//!
//! All the placeholders are numbers without a unit.
//!
//! Placeholder        | Value
//! -------------------|------
//! `total`            | The total number of notifications
//! `assign`           | You were assigned to the issue
//! `author`           | You created the thread
//! `comment`          | You commented on the thread
//! `ci_activity`      | A GitHub Actions workflow run that you triggered was completed
//! `invitation`       | You accepted an invitation to contribute to the repository
//! `manual`           | You subscribed to the thread (via an issue or pull request)
//! `mention`          | You were specifically @mentioned in the content
//! `review_requested` | You, or a team you're a member of, were requested to review a pull request
//! `security_alert`   | GitHub discovered a security vulnerability in your repository
//! `state_change`     | You changed the thread state (for example, closing an issue or merging a pull request)
//! `subscribed`       | You're watching the repository
//! `team_mention`     | You were on a team that was mentioned
//!
//! # Examples
//!
//! ```toml
//! [[block]]
//! block = "github"
//! format = "$total.eng(1)|$mention.eng(1)"
//! interval = 60
//! token = "..."
//! ```
//!
//! ```toml
//! [[block]]
//! block = "github"
//! token = "..."
//! format = "$total.eng(1)"
//! info = ["total"]
//! warning = ["mention","review_requested"]
//! hide_if_total_is_zero = true
//! ```
//!
//! # Icons Used
//! - `github`

use std::collections::HashMap;

use super::prelude::*;

#[derive(Deserialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
struct GithubConfig {
    #[derivative(Default(value = "60.into()"))]
    interval: Seconds,
    format: FormatConfig,
    token: Option<StdString>,
    hide_if_total_is_zero: bool,
    good: Option<Vec<String>>,
    info: Option<Vec<String>>,
    warning: Option<Vec<String>>,
    critical: Option<Vec<String>>,
}

pub async fn run(config: toml::Value, mut api: CommonApi) -> Result<()> {
    let config = GithubConfig::deserialize(config).config_error()?;
    api.set_format(config.format.with_default("$total.eng(1)")?);
    api.set_icon("github")?;

    let mut interval = config.interval.timer();
    let token = match config.token {
        Some(token) => token,
        None => match std::env::var("I3RS_GITHUB_TOKEN") {
            Ok(var) => var,
            Err(_) => return Err(Error::new("Github token not found")),
        },
    };

    loop {
        let stats = api.recoverable(|| get_stats(&token), "X").await?;
        if stats.get("total").map_or(false, |x| *x > 0) || !config.hide_if_total_is_zero {
            let mut state = State::Idle;
            'outer: for (list_opt, ret) in [
                (&config.critical, State::Critical),
                (&config.warning, State::Warning),
                (&config.info, State::Info),
                (&config.good, State::Good),
            ] {
                if let Some(list) = list_opt {
                    for val in list {
                        if stats.get(val).map_or(false, |x| *x > 0) {
                            state = ret;
                            break 'outer;
                        }
                    }
                }
            }
            let stats: HashMap<_, _> = stats
                .into_iter()
                .map(|(k, v)| (k, Value::number(v)))
                .collect();
            api.set_state(state);
            api.set_values(stats);
            api.show();
        } else {
            api.hide();
        }
        api.flush().await?;
        interval.tick().await;
    }
}

#[derive(Deserialize, Debug)]
struct Notification {
    reason: String,
}

async fn get_stats(token: &str) -> Result<HashMap<String, usize>> {
    let mut stats = HashMap::new();
    let mut total = 0;
    for page in 1..100 {
        let on_page = get_on_page(token, page).await?;
        if on_page.is_empty() {
            break;
        }
        total += on_page.len();
        for n in on_page {
            stats.entry(n.reason).and_modify(|x| *x += 1).or_insert(1);
        }
    }
    stats.insert("total".into(), total);
    stats.entry("total".into()).or_insert(0);
    stats.entry("assign".into()).or_insert(0);
    stats.entry("author".into()).or_insert(0);
    stats.entry("comment".into()).or_insert(0);
    stats.entry("ci_activity".into()).or_insert(0);
    stats.entry("invitation".into()).or_insert(0);
    stats.entry("manual".into()).or_insert(0);
    stats.entry("mention".into()).or_insert(0);
    stats.entry("review_requested".into()).or_insert(0);
    stats.entry("security_alert".into()).or_insert(0);
    stats.entry("state_change".into()).or_insert(0);
    stats.entry("subscribed".into()).or_insert(0);
    stats.entry("team_mention".into()).or_insert(0);
    Ok(stats)
}

async fn get_on_page(token: &str, page: usize) -> Result<Vec<Notification>> {
    // https://docs.github.com/en/rest/reference/activity#notifications
    let request = REQWEST_CLIENT
        .get(format!(
            "https://api.github.com/notifications?per_page=100&page={}",
            page
        ))
        .header("Authorization", format!("token {}", token));
    request
        .send()
        .await
        .error("Failed to send request")?
        .json()
        .await
        .error("Failed to get JSON")
}
