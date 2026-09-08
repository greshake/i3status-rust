pub mod i3bar_block;
pub mod i3bar_event;

use std::borrow::Borrow;

use crate::RenderedBlock;
use crate::config::SharedConfig;
use crate::themes::color::Color;
use crate::themes::separator::Separator;

use i3bar_block::I3BarBlock;

pub fn init(never_pause: bool) {
    if never_pause {
        println!("{{\"version\": 1, \"click_events\": true, \"stop_signal\": 0}}\n[");
    } else {
        println!("{{\"version\": 1, \"click_events\": true}}\n[");
    }
}

pub(crate) fn print_blocks<B>(blocks: &[B], config: &SharedConfig)
where
    B: Borrow<RenderedBlock>,
{
    let rendered_blocks = render_blocks(blocks, config);
    println!("{},", serde_json::to_string(&rendered_blocks).unwrap());
}

/// Separators are rendered *between* blocks, so they cannot be styled by any
/// single block's widget rendering. Each separator takes its settings
/// (`separator`, `separator_fg`, `separator_bg`, `start_separator`) from the
/// theme of the block it precedes; `end_separator` takes them from the last
/// block's theme. Blocks without `theme_overrides` carry the global theme, so
/// this reduces to the old behavior when no per-block overrides are set.
fn render_blocks<B>(blocks: &[B], config: &SharedConfig) -> Vec<I3BarBlock>
where
    B: Borrow<RenderedBlock>,
{
    let mut prev_last_bg = Color::None;
    let mut prev_theme: Option<std::sync::Arc<crate::themes::Theme>> = None;
    let mut rendered_blocks = vec![];

    // The right most block should never be alternated
    let mut alt = blocks
        .iter()
        .map(|x| x.borrow())
        .filter(|x| !x.segments.is_empty() && !x.merge_with_next)
        .count()
        % 2
        == 0;

    let mut logical_block_i = 0;

    let mut prev_merge_with_next = false;

    for (i, widgets) in blocks
        .iter()
        .map(|x| x.borrow())
        .filter(|x| !x.segments.is_empty())
        .cloned()
        .enumerate()
    {
        let RenderedBlock {
            mut segments,
            merge_with_next,
            theme,
        } = widgets;

        for segment in &mut segments {
            segment.name = Some(logical_block_i.to_string());

            // Apply tint for all widgets of every second block
            // TODO: Allow for other non-additive tints
            if alt {
                segment.background = segment.background + config.theme.alternating_tint_bg;
                segment.color = segment.color + config.theme.alternating_tint_fg;
            }
        }

        if !merge_with_next {
            alt = !alt;
        }

        let separator = match &theme.start_separator {
            Separator::Custom(_) if i == 0 => &theme.start_separator,
            _ => &theme.separator,
        };

        if let Separator::Custom(separator) = separator {
            if !prev_merge_with_next {
                // The first widget's BG is used to get the FG color for the current separator
                let sep_fg = if theme.separator_fg == Color::Auto {
                    segments.first().unwrap().background
                } else {
                    theme.separator_fg
                };

                // The separator's BG is the last block's last widget's BG
                let sep_bg = if theme.separator_bg == Color::Auto {
                    prev_last_bg
                } else {
                    theme.separator_bg
                };

                let separator = I3BarBlock {
                    full_text: separator.clone(),
                    background: sep_bg,
                    color: sep_fg,
                    ..Default::default()
                };

                rendered_blocks.push(separator);
            }
        } else if !merge_with_next {
            // Re-add native separator on last widget for native theme
            segments.last_mut().unwrap().separator = None;
            segments.last_mut().unwrap().separator_block_width = None;
        }

        if !merge_with_next {
            logical_block_i += 1;
        }

        prev_merge_with_next = merge_with_next;
        prev_last_bg = segments.last().unwrap().background;
        prev_theme = Some(theme);

        rendered_blocks.extend(segments);
    }

    let end_theme = prev_theme.as_deref().unwrap_or(&config.theme);
    if let Separator::Custom(end_separator) = &end_theme.end_separator {
        // The separator's FG is the last block's last widget's BG
        let sep_fg = if end_theme.separator_fg == Color::Auto {
            prev_last_bg
        } else {
            end_theme.separator_fg
        };

        // The separator has no background color
        let sep_bg = if end_theme.separator_bg == Color::Auto {
            Color::None
        } else {
            end_theme.separator_bg
        };

        let separator = I3BarBlock {
            full_text: end_separator.clone(),
            background: sep_bg,
            color: sep_fg,
            ..Default::default()
        };

        rendered_blocks.push(separator);
    }

    rendered_blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::Theme;
    use std::str::FromStr as _;
    use std::sync::Arc;

    fn theme_with(f: impl FnOnce(&mut Theme)) -> Arc<Theme> {
        let mut theme = Theme::default();
        f(&mut theme);
        Arc::new(theme)
    }

    fn block(text: &str, theme: &Arc<Theme>) -> RenderedBlock {
        RenderedBlock {
            segments: vec![I3BarBlock {
                full_text: text.into(),
                ..Default::default()
            }],
            merge_with_next: false,
            theme: theme.clone(),
        }
    }

    #[test]
    fn per_block_separator_overrides() {
        let config = SharedConfig::default();
        let theme_a = theme_with(|t| {
            t.separator = Separator::Custom("|A|".into());
        });
        let theme_b = theme_with(|t| {
            t.separator = Separator::Custom("|B|".into());
            t.separator_fg = Color::from_str("#123456").unwrap();
        });

        let out = render_blocks(&[block("a", &theme_a), block("b", &theme_b)], &config);

        let texts: Vec<&str> = out.iter().map(|b| b.full_text.as_str()).collect();
        assert_eq!(texts, ["|A|", "a", "|B|", "b"]);
        // the separator preceding a block takes that block's theme
        assert_eq!(out[2].color, Color::from_str("#123456").unwrap());
    }

    #[test]
    fn per_block_start_and_end_separators() {
        let config = SharedConfig::default();
        let first = theme_with(|t| {
            t.start_separator = Separator::Custom("<start>".into());
        });
        let last = theme_with(|t| {
            t.end_separator = Separator::Custom("<end>".into());
        });

        let out = render_blocks(&[block("a", &first), block("b", &last)], &config);

        let texts: Vec<&str> = out.iter().map(|b| b.full_text.as_str()).collect();
        assert_eq!(texts, ["<start>", "a", "b", "<end>"]);
    }

    #[test]
    fn global_theme_when_no_overrides() {
        let config = SharedConfig::default();
        let theme = Arc::new(Theme::default());

        let out = render_blocks(&[block("a", &theme), block("b", &theme)], &config);

        // default theme uses native separators: no separator segments injected
        let texts: Vec<&str> = out.iter().map(|b| b.full_text.as_str()).collect();
        assert_eq!(texts, ["a", "b"]);
    }

    /// A global custom separator, one block overriding it (with its own fg),
    /// and the last block providing end_separator — mirrors a live-tested
    /// configuration.
    #[test]
    fn mixed_global_and_block_overrides() {
        let global = theme_with(|t| {
            t.separator = Separator::Custom("|G|".into());
        });
        let config = SharedConfig {
            theme: global.clone(),
            ..Default::default()
        };
        // per-block themes start as a copy of the global theme with the
        // block's overrides applied on top, as in BarState::spawn_block
        let theme_a = theme_with(|t| {
            t.separator = Separator::Custom(">>A".into());
            t.separator_fg = Color::from_str("#ff0000").unwrap();
        });
        let theme_c = theme_with(|t| {
            t.separator = Separator::Custom("|G|".into());
            t.end_separator = Separator::Custom("<<END".into());
        });

        let out = render_blocks(
            &[
                block("A", &theme_a),
                block("B", &global),
                block("C", &theme_c),
            ],
            &config,
        );

        let texts: Vec<&str> = out.iter().map(|b| b.full_text.as_str()).collect();
        assert_eq!(texts, [">>A", "A", "|G|", "B", "|G|", "C", "<<END"]);
        assert_eq!(out[0].color, Color::from_str("#ff0000").unwrap());
    }
}
