# i3status-rust
Very resourcefriendly and feature-rich replacement for i3status, written in pure Rust

# About this project
This is a project I developed mostly because I wanted to train my Rust fluency. There are plenty more mature implementations out there. This one has some unique features however:
- configurable update times for each block -> less system calls
- support of click actions  (WiP)
- theming

NOTE: Currently, this program is configured in source code, however I plan to change that.
      Also, there are very little modules and themes right now, because I am still in the process of implementing those.

# How to use it
1. Clone the repository
2. Configure the program by editing src/main.rs. Documentation is sparse right now, but so are the features.
3. run cargo build
4. in your i3 config, put the path to the output binary as argument for 'status_command'
5. reload i3
