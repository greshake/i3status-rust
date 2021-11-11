pub mod i3bar_block;
pub mod i3bar_event;

use crate::blocks::Block;
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

pub fn print_blocks(blocks: &[Box<dyn Block>], config: &SharedConfig) -> Result<()> {
    let mut last_bg = Color::None;

    let mut rendered_blocks = vec![];

    /* To always start with the same alternating tint on the right side of the
     * bar it is easiest to calculate the number of visible blocks here and
     * flip the starting tint if an even number of blocks is visible. This way,
     * the last block should always be untinted.
     */
    let visible_count = blocks
        .iter()
        .filter(|block| !block.view().is_empty())
        .count();

    let mut alternator = visible_count % 2 == 0;

    for block in blocks.iter() {
        let widgets = block.view();
        if widgets.is_empty() {
            continue;
        }

        let mut rendered_widgets: Vec<I3BarBlock> = widgets
            .iter()
            .map(|widget| {
                let mut data = widget.get_data();
                if alternator {
                    // Apply tint for all widgets of every second block
                    // TODO: Allow for other non-additive tints
                    data.background = data.background + config.theme.alternating_tint_bg;
                    data.color = data.color + config.theme.alternating_tint_fg;
                }
                data
            })
            .collect();

        alternator = !alternator;

        if config.theme.separator.is_none() {
            // Re-add native separator on last widget for native theme
            rendered_widgets.last_mut().unwrap().separator = None;
            rendered_widgets.last_mut().unwrap().separator_block_width = None;
        }

        // Serialize and concatenate widgets
        let block_str = rendered_widgets
            .iter()
            .map(|w| w.render())
            .collect::<Vec<String>>()
            .join(",");

        if config.theme.separator.is_none() {
            // Skip separator block for native theme
            rendered_blocks.push(block_str.to_string());
            continue;
        }

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

        if let Some(ref separator) = config.theme.separator {
            let separator = I3BarBlock {
                full_text: separator.clone(),
                background: sep_bg,
                color: sep_fg,
                ..Default::default()
            };
            rendered_blocks.push(format!("{},{}", separator.render(), block_str));
        } else {
            rendered_blocks.push(block_str);
        }

        // The last widget's BG is used to get the BG color for the next separator
        last_bg = rendered_widgets.last().unwrap().background;
    }

    println!("[{}],", rendered_blocks.join(","));

    Ok(())
}
