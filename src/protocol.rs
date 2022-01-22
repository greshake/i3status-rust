pub mod i3bar_block;
pub mod i3bar_event;

use crate::config::SharedConfig;
use crate::errors::*;
use crate::themes::Color;

use i3bar_block::I3BarBlock;

pub fn init(never_pause: bool) {
    if never_pause {
        println!("{{\"version\": 1, \"click_events\": true, \"stop_signal\": 0}}\n[");
    } else {
        println!("{{\"version\": 1, \"click_events\": true}}\n[");
    }
}

pub fn print_blocks(blocks: &[Vec<I3BarBlock>], config: &SharedConfig) -> Result<()> {
    let mut last_bg = Color::None;

    let mut rendered_blocks = vec![];

    // The right most block should never be alternated
    let mut alt = true;
    for x in blocks.iter() {
        if !x.is_empty() {
            alt = !alt;
        }
    }

    for widgets in blocks.iter() {
        if widgets.is_empty() {
            continue;
        }

        let mut rendered_widgets = widgets.clone();

        // Apply tint for all widgets of every second block
        // TODO: Allow for other non-additive tints
        if alt {
            for data in &mut rendered_widgets {
                data.background = data.background + config.theme.alternating_tint_bg;
                data.color = data.color + config.theme.alternating_tint_fg;
            }
        }
        alt = !alt;

        if let Some(separator) = &config.theme.separator {
            // The first widget's BG is used to get the FG color for the current separator
            let sep_fg = if config.theme.separator_fg == Color::Auto {
                rendered_widgets.first().unwrap().background
            } else {
                config.theme.separator_fg
            };

            // The separator's BG is the last block's last widget's BG
            let sep_bg = if config.theme.separator_bg == Color::Auto {
                last_bg
            } else {
                config.theme.separator_bg
            };

            // The last widget's BG is used to get the BG color for the next separator
            last_bg = rendered_widgets.last().unwrap().background;

            let separator = I3BarBlock {
                full_text: separator.clone().into(),
                background: sep_bg,
                color: sep_fg,
                ..Default::default()
            };

            rendered_blocks.push(separator);
            rendered_blocks.extend(rendered_widgets);
        } else {
            // Re-add native separator on last widget for native theme
            rendered_widgets.last_mut().unwrap().separator = None;
            rendered_widgets.last_mut().unwrap().separator_block_width = None;

            rendered_blocks.extend(rendered_widgets);
        }
    }

    println!("{},", serde_json::to_string(&rendered_blocks).unwrap());

    Ok(())
}
