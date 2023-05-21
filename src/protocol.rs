pub mod i3bar_block;
pub mod i3bar_event;

use std::borrow::Borrow;

use crate::config::SharedConfig;
use crate::themes::color::Color;
use crate::themes::separator::Separator;
use crate::RenderedBlock;

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
    let mut prev_last_bg = Color::None;
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

    for widgets in blocks
        .iter()
        .map(|x| x.borrow())
        .filter(|x| !x.segments.is_empty())
        .cloned()
    {
        let RenderedBlock {
            mut segments,
            merge_with_next,
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

        if let Separator::Custom(separator) = &config.theme.separator {
            if !prev_merge_with_next {
                // The first widget's BG is used to get the FG color for the current separator
                let sep_fg = if config.theme.separator_fg == Color::Auto {
                    segments.first().unwrap().background
                } else {
                    config.theme.separator_fg
                };

                // The separator's BG is the last block's last widget's BG
                let sep_bg = if config.theme.separator_bg == Color::Auto {
                    prev_last_bg
                } else {
                    config.theme.separator_bg
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

        rendered_blocks.extend(segments);
    }

    if let Separator::Custom(end_separator) = &config.theme.end_separator {
        rendered_blocks.push(I3BarBlock {
            full_text: end_separator.clone(),
            background: Color::None,
            color: prev_last_bg,
            ..Default::default()
        });
    }

    println!("{},", serde_json::to_string(&rendered_blocks).unwrap());
}
