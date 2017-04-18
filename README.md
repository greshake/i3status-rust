# i3status-rust
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
      2. Add the path to your config file as first argument, you can also configure theme and icon theme as arguments to i3bar-rs. See i3bar-rs --help for more.
      
            Example of the 'bar' block in the i3 config from my personal i3 config (Requires awesome-fonts and powerline-fonts). The colors block is optional, just my taste:

            ```
            bar {
                  font xft:DejaVu Sans Mono for Powerline, Icons 12
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

Options:

format: String, Format string. Default is "%a %d/%m %R". See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options.

## Music
Creates a block which can display the current song title and artist, in a fixed width rotating-text fashion. It uses dbus signaling to fetch new tracks, so no periodic updates are needed. It supports all Players that implement the [MediaPlayer2 Interface](https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html). This includes spotify, vlc and many more.

Options:

player: String, e.g. "spotify"

### Music Play/Pause
Optional Play/Pause block, works similar to the Music block, displays a Play/Pause button.

Options:

player: String, e.g. "spotify"

# How to write a Block

## Step 1: Create the file

Create a block by copying the template: `cp src/blocks/template.rs src/blocks/<block_name>.rs` Obviously, you have to be in the main repo directory and replace <block_name> with the name of your block.

## Step 2: Populate the struct

Your block needs a struct to store it's state. First, replace all the occurences of 'Template' in the file with the name of your block. Then edit the struct and add all Fields which you may need to store either options from the block config or state values (e.g. free disk space or current load). All Blocks use interior mutability to update their state. For primitive data types, use a field of type Cell<T>, for Strings and complex types use RefCell<T>.

## Step 3: Implement the constructor

You now need to write a constructor (new()) to create your Block from a piece of JSON (from the config file section of your block). Access values from the config here with config["name"], then use .as_str() or as_u64() to convert the argument to the right type, and unwrap it with expect() or unwrap_or() to give it a default value. Alternatively, you can use the helper macros get_str/u64 to extract a string/ u64 and add appropriate error handeling. You can set a default value in the macro as you can see below.

Example:
```rust
pub fn new(config: Value) -> Template {
      Template {
            name: get_str(config, "name"),
            update_interval: Duration::new(get_u64_default!(config, "interval", 5), 0),

            some_value: RefCell::new(get_str_default!(config, "hello", "Default is Hello World")),
            click_count: Cell::new(0),
      }
}
```

## Step 4: Implement the Block interface

All blocks are basically structs which implement the trait (interface) Block. This interface defines the following features:

### `fn get_status(&self, theme: &Value) -> Value` (Required)

Use this function to render the content of your Block to a i3bar compatible json value. **Note**: Do not execute any commands/system calls here. All the heavy lifting is supposed to be done in the update() method. Also, you get access to the theme (JSON Value). Use it to extract icons or colors if needed. Otherwise, the colors will be rendered on top of the returned JSON. State colors from get_state() are also applied automatically.

Example:
```rust
fn get_status(&self, _: &Value) -> Value {
      json!({
            "full_text": format!("{}{}", theme["icons"]["time"].as_str().unwrap(),
                                           self.time.clone().into_inner())
      })
}
```

### `fn get_state(&self) -> State` (Optional) 

Use this function to return a general representation of your Block's state. This general state is then translated into color based on the current theme. Again, please don't update the internal state of the block here!

Example:
```rust
fn get_state(&self) -> State {
        match self.some_value.get() {
            0 ... 10 => State::Critical,
            10 ... 20 => State::Warning,
            _ => State::Good,
        }
    }
```

### `fn update(&self) -> Option<Duration>` (Optional, but probably recommended)

Use this function to update the internal state of your block in a specified interval. For example, update the free disk space. i3status-rs tries to call this method as little as possible, to avoid unnessesary system calls. If you return None, the block will not be automatically updated again. This is the default behaviour. This may be useful to blocks which are static or updating in an event guided manner (maybe from a seperate thread). Otherwise, this method will be called again after the specified duration.

Example:
```rust
fn update(&self) -> Option<Duration> {
      match self.info_type {
            DiskInfoType::Available => {
                  let statvfs = Statvfs::for_path(Path::new(self.target)).unwrap();
                  let available = self.unit.convert_bytes(statvfs.f_bavail * statvfs.f_bsize);
                  self.value.set(available);
            }
            DiskInfoType::Free => {
                  let statvfs = Statvfs::for_path(Path::new(self.target)).unwrap();
                  let free = self.unit.convert_bytes(statvfs.f_bfree * statvfs.f_bsize);
                  self.value.set(free);
            }
            _ => unimplemented!(),
      }
      Some(Duration::new(5, 0))
}
```

### `fn id(&self) -> Option<&str>` (Optional, but required if you want to react to clicks)

Use this function to return a unique identifier for your block. It is required if you also implement the click funtion, because thats how i3bar identifies clicked blocks. Best practice is to return a unique identifier here, that was randomly created when the block was created, and is static over the Block's lifetime. Otherwise, you may also let it be user definable in the config; it should, however, not be required.

Example:
```rust
fn id(&self) -> Option<&str> {
      Some(&self.name)
}
```

### `fn click(&self, I3barEvent)` (Optional)

Here you can react to the user clicking your block. The i3barEvent instance contains all fields to describe the click action, including mouse button and location down to the pixel. You may also update the internal state here.

Example:
```rust
fn click(&self, event: I3barEvent) {
      match event.button {
      1 => { // Left mouse button
            let old = self.click_count.get();
            let new: u32 = old + 1;
            self.click_count.set(new);
            *self.some_value.borrow_mut() = format!("Click Count: {}", new);
      }
      3 => { // Right mouse button
            let old = self.click_count.get();
            let new: u32 = if old > 0 { old - 1 } else { 0 };
            self.click_count.set(new);
            *self.some_value.borrow_mut() = format!("Click Count: {}", new);
      }
      _ => {}
      }
}
```

## Step 5: Register your Block

Edit `src/blocks/mod.rs` and add:
1. A module export line:      `pub mod <name>;`
2. A use directive:           `use self::<name>::*;`
3. Mapping to a name string:  `"<name>" => boxed!(<name>::new(config)),`

**Congratulations** You're done. Recompile and just add the block to your config file now.

# ToDo
- further documentation in the source code
- more caching

## Blocks to be implemented
- CPU
- Load
- Battery
- Disk Space
- Memory
- Pacman updates
- Sound
  * Maybe features like click-to-mute
- Network
- NetworkManager with Dbus
- open to more ideas