//! The number of GitHub notifications
//!
//! This block shows the unread notification count for a GitHub account. A GitHub [personal access token](https://github.com/settings/tokens/new) with the "notifications" scope is required, and must be passed using the `I3RS_GITHUB_TOKEN` environment variable or `token` configuration option. Optionally the colour of the block is determined by the highest notification in the following lists from highest to lowest: `critical`,`warning`,`info`,`good`
//!
//! # Configuration
//!
//! Key | Values | Default
//! ----|--------|--------
//! `format` | A string to customise the output of this block. See below for available placeholders. | `" $icon $total.eng(w:1) "`
//! `interval` | Update interval in seconds | `30`
//! `token` | A GitHub personal access token with the "notifications" scope | `None`
//! `hide_if_total_is_zero` | Hide this block if the total count of notifications is zero | `false`
//! `critical` | List of notification types that change the block to the critical colour | `None`
//! `warning` | List of notification types that change the block to the warning colour | `None`
//! `info` | List of notification types that change the block to the info colour | `None`
//! `good` | List of notification types that change the block to the good colour | `None`
//!
//!
//! All the placeholders are numbers without a unit.
//!
//! Placeholder        | Value
//! -------------------|------
//! `icon`             | A static icon
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
//! format = " $icon $total.eng(1)|$mention.eng(1) "
//! interval = 60
//! token = "..."
//! ```
//!
//! ```toml
//! [[block]]
//! block = "github"
//! token = "..."
//! format = " $icon $total.eng(1) "
//! info = ["total"]
//! warning = ["mention","review_requested"]
//! hide_if_total_is_zero = true
//! ```
//!
//! # Icons Used
//! - `github`

use super::prelude::*;

#[derive(Deserialize, Debug, SmartDefault)]
#[serde(default)]
pub struct Config {
    #[default(60.into())]
    interval: Seconds,
    format: FormatConfig,
    token: Option<String>,
    hide_if_total_is_zero: bool,
    good: Option<Vec<String>>,
    info: Option<Vec<String>>,
    warning: Option<Vec<String>>,
    critical: Option<Vec<String>>,
}

pub async fn run(config: Config, mut api: CommonApi) -> Result<()> {
    let mut widget =
        Widget::new().with_format(config.format.with_default(" $icon $total.eng(w:1) ")?);

    let mut interval = config.interval.timer();
    let token = config
        .token
        .or_else(|| std::env::var("I3RS_GITHUB_TOKEN").ok())
        .error("Github token not found")?;

    loop {
        let stats = api.recoverable(|| get_stats(&token)).await?;
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
            let mut values: HashMap<_, _> = stats
                .into_iter()
                .map(|(k, v)| (k.into(), Value::number(v)))
                .collect();
            values.insert("icon".into(), Value::icon(api.get_icon("github")?));
            widget.set_values(values);
            widget.state = state;
            api.set_widget(&widget).await?;
        } else {
            api.hide().await?;
        }

        select! {
            _ = interval.tick() => (),
            _ = api.wait_for_update_request() => (),
        }
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
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Response {
        Notifications(Vec<Notification>),
        ErrorMessage { message: String },
    }

    // https://docs.github.com/en/rest/reference/activity#notifications
    let request = REQWEST_CLIENT
        .get(format!(
            "https://api.github.com/notifications?per_page=100&page={page}",
        ))
        .header("Authorization", format!("token {token}"));
    let responce = request
        .send()
        .await
        .error("Failed to send request")?
        .json::<Response>()
        .await
        .error("Failed to get JSON")?;

    match responce {
        Response::Notifications(n) => Ok(n),
        Response::ErrorMessage { message } => Err(Error::new(format!("API error: {message}"))),
    }
}
