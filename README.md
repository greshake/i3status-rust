# i3status-rust 
![demo1](https://raw.githubusercontent.com/XYunknown/i3status-rust/master/img/example_bar.png)

Very resourcefriendly and feature-rich replacement for i3status, written in pure Rust

# About this project
This is a WiP replacement for i3status, aiming to provide the most feature-complete and resource friendly implementation of the i3bar protocol availiable. We are currently looking for help in implementing more Blocks. It supports:
- flexibility through theming
- icons (optional)
- individual update intervals per block to reduce system calls
- click actions
- blocks can trigger updates asynchronously, which allows for things like dbus signaling, to avoid periodic refreshing of data that rarely changes (example: music block)

# Requirements
i3, rustc and cargo. Only tested on Arch Linux. If you want to use the font icons on Arch, install ttf-font-awesome from the AUR.

# How to use it
1. Clone the repository: `git clone https://github.com/XYunknown/i3status-rust.git`
2. run `cd i3status-rust && cargo build --release`
3. Edit example_config.json to your liking and put it to a sensible place (e.g. ~/.config/i3/status.json)
4. Edit your i3 config
      1. In your i3 config, put the path to the output binary as argument for 'status_command'
      2. Add the path to your config file as first argument, you can also configure theme and icon theme as arguments to i3status-rs. See i3status-rs --help for more.
      
            Example of the 'bar' section in the i3 config from my personal i3 config (Requires awesome-ttf-fonts). The colors block is optional, just my taste:

            ```
            bar {
                  font pango:DejaVu Sans Mono, Icons 12
                  position top
                  status_command <PATH_TO_i3STATUS>/i3status-rs <PATH_TO_CONFIG>/config.json --icons awesome --theme solarized-dark
                  colors {
                        separator #666666
                        background #222222
                        statusline #dddddd
                        focused_workspace #0088CC #0088CC #ffffff
                        active_workspace #333333 #333333 #ffffff
                        inactive_workspace #333333 #333333 #888888
                        urgent_workspace #2f343a #900000 #ffffff
                  }
            }
            ```
5. Reload i3: `i3 reload`

# Available Blocks
## Time
Creates a block which display the current time.

**Example**
```javascript
{"block": "time", "interval": 60, "format": "%a %d/%m %R"},
```
**Options**

Key | Values | Required | Default
----|--------|----------|--------
format | Format string.<br/> See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options. | No | %a %d/%m %R
interval | Update interval in seconds | No | 5

## Memory
Creates a block displaying memory and swap usage.
By default, the format of this module is "<Icon>: {MFm}MB/{MTm}MB({Mp}%)" (Swap values
accordingly). That behaviour can be changed within config.json.
This module keeps track of both Swap and Memory. By default, a click switches between them.

**Example**

```javascript
{"block": "memory",
    "format_mem": "{MFm}MB/{MTm}MB({Mp}%)", "format_swap": "{SFm}MB/{STm}MB({Sp}%)",
    "type": "memory", "icons": "true", "clickable": "true", "interval": "5"
},
```


**Options**

Key | Values | Required | Default
----|--------|----------|--------
format_mem | Format string for Memory view. All format values are described below. | No | `{MFm}MB/{MTm}MB({Mp}%)`
format_swap | Format string for Swap view. | No | `{SFm}MB/{STm}MB({Sp}%)`
type | Default view displayed on startup. Options are <br/> memory, swap | No | memory
icons | Whether the format string should be prepended with Icons. Options are <br/> true, false | No | true
clickable | Whether the view should switch between memory and swap on click. Options are <br/> true, false | No | true
interval | The delay in seconds between an update. If `clickable`, an update is triggered on click. Integer values only. | No | 5

### Format string specification

Key | Values
----|-------
{MTg} | Memory total (GiB)
{MTm} | Memory total (MiB)
{MFg} | Memory free (GiB)
{MFm} | Memory free (MiB)
{Mp} | Memory used (%)
{STg} | Swap total (GiB)
{STm} | Swap total (MiB)
{SFg} | Swap free (GiB)
{SFm} | Swap free (MiB)
{Sp} | Swap used (%)

## Music
Creates a block which can display the current song title and artist, in a fixed width marquee fashion. It uses dbus signaling to fetch new tracks, so no periodic updates are needed. It supports all Players that implement the [MediaPlayer2 Interface](https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html). This includes spotify, vlc and many more. Also provides buttons for play/pause, previous and next title.

**Example**
```javascript
{"block": "music", "player": "spotify", "buttons": ["play", "next"]},
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
player | Name of the music player.Must be the same name the player<br/> is registered with the MediaPlayer2 Interface.  | Yes | -
max_width | Max width of the block in characters, not including the buttons | No | 21
marquee | Bool to specify if a marquee style rotation should be used every<br/>10s if the title + artist is longer than max-width | No | true
buttons | Array of control buttons to be displayed. Options are<br/>prev (previous title), play (play/pause) and next (next title) | No | []

## Load
Creates a block which displays the system load average.

**Example**
```javascript
{"block": "load", "format": "{1m} {5m}", "interval": 1},
```
**Options**

Key | Values | Required | Default
----|--------|----------|--------
format | Format string.<br/> You can use the placeholders 1m 5m and 15m, eg "1min avg: {1m}" | No | {1m}
interval | Update interval in seconds | No | 3

## Cpu utilization
Creates a block which displays the overall CPU utilization, calculated from /proc/stat.

**Example**
```javascript
{"block": "cpu", "interval": 1},
```
**Options**

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval in seconds | No | 1

## Battery
Creates a block which displays the current battery state (Full, Charging or Discharging) and percentage charged.

**Example**
```javascript
{"block": "battery", "interval": 10},
```
**Options**

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval in seconds | No | 10
device | Which BAT device in /sys/class/power_supply/ to read from. | No | 0

## Pacman
Creates a block which displays the pending updates available on pacman.

**Example**
```javascript
{"block": "pacman"},
```

**Options**
There are no options available yet. If you need a specific option, file an issue.

# How to write a Block

## Step 1: Create the file

Create a block by copying the template: `cp src/blocks/template.rs src/blocks/<block_name>.rs` Obviously, you have to be in the main repo directory and replace <block_name> with the name of your block.

## Step 2: Populate the struct

Your block needs a struct to store it's state. First, replace all the occurences of 'Template' in the file with the name of your block. Then edit the struct and add all Fields which you may need to store either options from the block config or state values (e.g. free disk space or current load). Use Widgets to display something in the i3Bar, you can have multiple Text or Button widgets on a Block. These have to be returned in the view() function and they need to be updated from the update() function. They also handle icons and theming for you.

## Step 3: Implement the constructor

You now need to write a constructor (new()) to create your Block from a piece of JSON (from the config file section of your block). Access values from the config here with config["name"], then use .as_str() or as_u64() to convert the argument to the right type, and unwrap it with expect() or unwrap_or() to give it a default value. Alternatively, you can use the helper macros get_str/u64/bool to extract a string/ u64 and add appropriate error handeling. You can set a default value in the macro as you can see below. The template shows you how to instantiate a simple Text widget. For more info on how to use widgets, just look into other Blocks. More documentation to come. The sender object can be used to send asynchronous update request for any block from a separate thread, provide you know the Block's ID.This advanced feature can be used to reduce the number of system calls by asynchrounosly waiting for events. A usage example can be found in the Music block, which updates only when dbus signals a new song.

Example:
```rust
pub fn new(config: Value, tx: Sender<Task>, theme: Value) -> Template {
      let text = TextWidget::new(theme.clone()).with_text("I'm a Template!");
      Template {
            id: Uuid::new_v4().simple().to_string(),
            update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),
            text: text,
            tx_update_request: tx,
            theme: theme,
      }
}
```

## Step 4: Implement the Block interface

All blocks are basically structs which implement the trait (interface) Block. This interface defines the following features:

### `fn update(&mut self) -> Option<Duration>` (Required if you don't want a static block)

Use this function to update the internal state of your block, for example during periodic updates. Return the duration until your block wants to be updated next. For example, a clock could request only to be updated every 60 seconds by returning Some(Duration::new(60, 0)) every time. If you return None, this function will not be called again automatically.

Example:
```rust
fn update(&mut self) -> Option<Duration> {
      self.time.set_text(format!("{}", Local::now().format(&self.format)));
      Some(self.update_interval.clone())
}
```

### `fn view(&self) -> Vec<&I3BarWidget>` (Required) 

Use this function to return the widgets that comprise the UI of your component. The music block may, for example, be comprised of a text widget and multiple buttons. Use a vec to wrap the references to your view.

Example:
```rust
fn view(&self) -> Vec<&I3BarWidget> {
      vec![&self.time]
}
```

### `fn id(&self) -> &str` (Required)

You need to return a unique identifier for your block here. In the template you will already find a UUID implementation being used here. This is needed, for example, to send update requests (callbacks) from a different thread.  

Example:
```rust
fn id(&self) -> &str {
      &self.id
}
```


### `fn click(&mut self, event: &I3barEvent)` (Optional)

Here you can react to the user clicking your block. The i3barEvent instance contains all fields to describe the click action, including mouse button and location down to the pixel. You may also update the internal state here. **Note that this event is sent to every block on every click**. *To filter, use the event.name property, which corresponds to the name property on widgets!*

Example:
```rust
if event.name.is_some() {
            let action = match &event.name.clone().unwrap() as &str {
                  "play" => "PlayPause",
                  "next" => "Next",
                  "prev" => "Previous",
                  _ => ""
            };
      }
}
```

## Step 5: Register your Block

Edit `src/blocks/mod.rs` and add:
1. A module export line:      `pub mod <name>;`
2. A use directive:           `use self::<name>::*;`
3. Mapping to a name string:  `"<name>" => boxed!(<name>::new(config)),`

**Congratulations** You're done. Recompile and just add the block to your config file now.
