# i3status-rust 0.21.6

### New Blocks and Features

* Hueshift: Add wl-gammarelay driver (#1421 by bim9262)
 
### Bug Fixes and Improvements

* Battery: prefer system batteries (BATx/CMBx) when doing auto discovery (3db119a5a2dd12a65a499377cf849d418bfee308)

# i3status-rust 0.21.5

### New Blocks and Features

* Add `if_command` field to block config to allow conditional enabling of blocks on startup (#1415 by LordMZTE) 
 
### Bug Fixes and Improvements

* Battery: revert to previous default device discovery behaviour (d6fbfd06cc4d078efccb1c559e7eb934d36ffe7a)

# i3status-rust 0.21.4

### Bug Fixes and Improvements

* Battery: fix issues with finding battery device paths (#1417 by @bim9262)
* Battery: better default values for `device` (c6824727020090bf6eb59cd3bf6f4de0f10179fa)

# i3status-rust 0.21.3

### Bug Fixes and Improvements

* Temperature: use libsensors bindings instead of sensors binary (#1375 by @MaxVerevkin)
* Hueshift: do not leave zombies (#1411 by @Naarakah)
* Time: reflect timezone changes (72a7284)
* Watson: fix automatic updates (0b810cb and 0b810cb)

### Deprecation Warnings
* Temperature: `sysfs` driver will be removed in a future release.

# i3status-rust 0.21.2

### New Blocks and Features

* Add dracula theme (#1408 by @welcoMattic)

### Bug Fixes and Improvements

* Battery block: Fix UPower property type mismatch (#1409 by @bim9262)

# i3status-rust 0.21.0

### New Blocks and Features

* New block: `rofication` (#1356 by @cfsmp3)
* New block: `external_ip` (#1366 by @cfsmp3)
* Xrandr block: new option `format` (it overrides `icons` and `resolution` options which are now deprecated) (ca86a97)
* Battery block: add new apcupsd driver (#1383 by @bim9262)
* Battery block: enable `allow_missing` for the UPower driver (#1378 by @bim9262)
* KeyboardLayout: add support for the xkb-switch keyboard layout reader (#1386 by @roguh)

### Bug Fixes and Improvements

* Sound block: fix headphones indicator (#1363 by @codicodi)
* Sound block: named PulseAudio devices now work as expected (#1394 by @bim9262)
* NetworkManager block: escape SSID (#1373 by @nzig)
* Taskwarrior block: use inotify to get instant changes (you will need to set `data_location` option if `taskwarrior` is configured to use a custom data directory) (#1374 by @cfsmp3)
* Battery block: fix spacing (#1389 by @bim9262)
* Hueshift block: replace `killall` with `pkill` (#1398 by @stelcodes)

### Deprecation Warnings
* Xrandr block: `icon` and `resolution` will be removed in a future release. Use `format` instead.
* Memory block: `icons` will be removed in a future release. Set `icons_format = ""` to disable icons.
* Maildir block: `icon` will be removed in a future release. Set `icons_format = ""` to disable icons.
* Notmuch block: `no_icon` will be removed in a future release. Set `icons_format = ""` to disable icons.

# i3status-rust 0.20.7

### New Blocks and Features

* Backlight block: new options `minimum`, `maximum`, `cycle` for toggling min/max brightness on click or on scrolll (#1349 by @Vanille-N)
* Focused Window block: add `format` string (#1360 by @cfsmp3)

### Bug Fixes and Improvements

* icons: Add missing bat_not_available icon (#1361 by @ram02z)
* Docker block: colour errors using Critical state (#1360 by @cfsmp3)

# i3status-rust 0.20.6

### New Blocks and Features

* Custom block: new `watch watch_files` option that uses inotify to trigger the block to update when one or more specified files are seen to have been modified (#1325 by @BrendanBall)
* CustomDBus block: new `initial_text` option to set the text shown up until the first update is received
* Hueshift block: added support for wlsunset (#1337 by @DerVerruckteFuchs)

### Bug Fixes and Improvements

* IBus block: no longer crashes the bar if IBus reports that there is no global engine set on first startup
* Music block: the default text icons are now pango escaped and should cause no errors with i3bar
 
# i3status-rust 0.20.5

### New Blocks and Features

* New DNF block for Fedora (#1311 by @sigvei)
* Docker block: allow non-default docker socket files (#1310 by @JTarasovic)
* Sound block: add option to automatically change icon based on output device type (#1313 by @codicodi)

### Bug Fixes and Improvements

* Hueshift block: fix sluggishness by updating widget text on interactions (#1320 by @JohnDowson)
* Music block: fix long standing issue where block tandomly stops updating (#1327 by jamesmcm)
* Nvidia block: fix nvidia block falling behind on lines from nvidia-smi (#1296 by @ZachCook)
 

# i3status-rust 0.20.4

### New Blocks and Features

* Github block: new config options `critical`, `warning`, `info`, `good` to colour the block for different notifications (#1286 by @ZachCook)
* Temperature block: new `driver` config option with the option to choose a new backend using sysfs to grab temp info instead of `lm_sensors` (#1286 by @ZachCook)

### Bug Fixes and Improvements

* Battery/Kdeconnect block: add more battery icons. For the new battery icons you will need to update your icon files, otherwise it will fallback to the previous icons. (#1282 by @freswa)
* Nvidia block: only run `nvidia-smi` once instead of spawning a new instance for each update (#1286 by @ZachCook)
* Weather block: escape spaces in internally generated URL (#1289 by @rbuch)

### Deprecation Warnings
`bat_half`, `bat_quarter`, `bat_three_quarters` are likely to be removed in a future release.

# i3status-rust 0.20.3

### Bug Fixes and Improvements

* Net block: fix SSID escape code decoding (#1274 by @GlasOSkar)
* NetworkManager block: update DBus interface for newer verisons of NM (#1269 by @mailhost)
* Pomodoro block: fix crash causing by pause icon typo (#1295 by @GladOSkar)
* Temperature block: fix fallback for users with old versions of `lm-sensors` (#1281 by @freswa)
* Icons: Fix `material-nf` icons that caused some blocks to render backwards (#1280 by @freswa)
* Themes: Add ability to unset colors using overrides (#1279 by @GlasOSkar and @MaxVerevkin)
* Themes: Fix alternating tint for the `slick` theme (#1284 by @MaxVerevkin)

If you are manually managing your icon/theme files then you may want to update them now for the above fixes.

# i3status-rust 0.20.2

### Bug Fixes and Improvements

* Battery block: find battery by default instead of hardcoding "BAT0" (#1258 by @orvij)
* Batter block: new `full_threshold` option for batteries that don't reach 100% (#1261 by @GladOSkar)
* CPU block: add `boost` format key for displaying CPU boost status (#1152 by @indlin)
* Custom block: better error message (#1233 by @jespino)
* Memory block: Count ZFS arc cache to cache to exclude from used memory (#1227 by @GladOSkar)
* Pacman block: fix invocation of fakeroot/pacman command (#1241)
* Pacman block: fix default format string (#1240 by @GladOSkar)
* Pomodoro block: Allow `notify-send` as a notification method
* Fixed missing net block icons for the material icon theme (#1244 by @K4rakara)
* Formatter: allow hiding unit prefixes. For example, `"{key;_K}"` will set the min unit prefix to "K" but hides it from showing.
* Formatter: allow spaces between the value and unit/prefix. For example, `"{key; K*b}"` results in "value Kb" and `"{key; _K*b}"` results in "val b".
* Add short_text support (#1207 by @GladOSkar)


### Breaking Changes

* Pomodoro block: Icons are no longer hardcoded. New icons: `pomodoro_started`, `pomodoro_stopped`, `pomodoro_paused`, `pomodoro_break` have been added to the icon themes in the repo, so you must update your icon theme files if it is not done by your package manager. (#1264)

### Deprecation Warnings
* Pomodoro block: `use_nag` and `nagbar_path` will be removed in a future release. Use `notifier` and `notifier_path` instead.

# i3status-rust 0.20.1

### Bug Fixes and Improvements

* Fixed config error messages showing in swaybar but not in i3bar (#1224 by @jthomaschewski)
* Fixed pacman block crash due to stderr output of pacman itself (#1220 by @mpldr)
* Custom block example list has been created and documented (#1223 by @GladOSkar)

# i3status-rust 0.20.0

### Breaking Changes

Themes/Icons:

* These have been moved out into files instead of being hardcoded in the Rust source. The following folders are checked in this order: first `$XDG_CONFIG_HOME/i3status-rust/<icons|themes>`, next `$HOME/.local/share/i3status-rust/<icons|themes>`, finally `/usr/share/i3status-rust/<icons|themes>`. If installing manually via cargo, you will need to copy the files to an appropriate location (an `install.sh` script is provided which does this). If installing via the AUR on Arch Linux, the package will install the files to `/usr/share/i3status-rust/<icons|themes>` for you, so you do not need to do anything (this should also be true for other distros assuming the package maintainer has packaged i3status-rust correctly). 

* Per block theme overrides have been renamed from `color_overrides` to `theme_overrides` (this was previously undocumented but has since been mentioned in themes.md)

Formatting:

* Formatting for all blocks using `format` strings has been overhauled to allow users to customise how numbers and strings are displayed, which was not possible previously. Due to this some blocks may now display slightly differently to previous versions and have been documented below. Refer to the [formatting documentation](doc/blocks.md#formatting) to get more information on the new formatting options.

Blocks: 

* CPU Utilization block: Due to an overhaul of our internal code, the `per_core` option has been removed. The same configuration can be achieved using the new `{utilization<n>}` format keys.
* Battery and Disk Space blocks: The `{bar}` format key has been removed in favor of the new [bar](doc/blocks.md#formatting#bar-max-value) formatter. For example, to make the Battery block display the current percentage as a 6 character bar with 100% as the max value, set the format string as so: `format = "{percentage:6#100}`. 
* Disk Space block: The `{unit}` format key has been removed since the unit of `{free}` and similar format keys don't rely on `unit` configuration option anymore.
* Maildir block: this is now optional and must be enabled at compile time (#1103 by @MaxVerevkin)
* Memory block: all old format keys have been removed, refer to the table below for more details.
* Net block: `use_bits`, `speed_min_unit`, `speed_digits` and `max_ssid_width` configuration options have been removed and require manual intervention to fix your config. `speed_min_unit` is replaced by the [min prefix](doc/blocks.md#min-prefix) formatter. `max_ssid_width` is replaced by the [max width](doc/blocks.md#0max-width) formatter.
* Net block: partially moved from calling external commands to using the netlink interface, which may not work on BSD systems (#1142 by @MaxVerevkin)
* Networkmanager block: `max_ssid_width` config option has been removed, but the bevaviour can be restored using the [max width](doc/blocks.md#max-width) formatter. For example, `max_ssid_width = 10` is now achieved with `ap_format = "{ssid^10}"`.
* Sound block: `max_width` config option has been removed, but the bevaviour can be restored using the [max width](doc/blocks.md#max-width) formatter.
* Speedtest block: `bytes`, `speed_min_unit` and `speed_digits` configuration options have been removed in favour of the new `format` string formatter. For example, to replicate `bytes=true; speed_min_unit="M", speed_digits=4` use `format = "{speed_down:4*B;M}{speed_up:4*B;M}"`

Memory block removed format keys:

 Old key | New alternative
---------|---------------
`{MTg}`  | `{mem_total;G}`
`{MTm}`  | `{mem_total;M}`
`{MAg}`  | `{mem_avail;G}`
`{MAm}`  | `{mem_avail;M}`
`{MAp}`  | `{mem_avail_percents}`
`{MApi}` | `{mem_avail_percents:1}`
`{MFg}`  | `{mem_free;G}`
`{MFm}`  | `{mem_free;M}`
`{MFp}`  | `{mem_free_percents}`
`{MFpi}` | `{mem_free_percents:1}`
`{Mug}`  | `{mem_used;G}`
`{Mum}`  | `{mem_used;M}`
`{Mup}`  | `{mem_used_percents}`
`{Mupi}` | `{mem_used_percents:1}`
`{MUg}`  | `{mem_total_used;G}`
`{MUm}`  | `{mem_total_used;M}`
`{MUp}`  | `{mem_total_used_percents}`
`{MUpi}` | `{mem_total_used_percents:1}`
`{Cg}`   | `{cached;G}`
`{Cm}`   | `{cached;M}`
`{Cp}`   | `{cached_percent}`
`{Cpi}`  | `{cached_percent:1}`
`{Bg}`   | `{buffers;G}`
`{Bm}`   | `{buffers;M}`
`{Bp}`   | `{buffers_percent}`
`{Bpi}`  | `{buffers_percent:1}`
`{STg}`  | `{swap_total;G}`
`{STm}`  | `{swap_total;M}`
`{SFg}`  | `{swap_free;G}`
`{SFm}`  | `{swap_free;M}`
`{SFp}`  | `{swap_free_percents}`
`{SFpi}` | `{swap_free_percents:1}`
`{SUg}`  | `{swap_used;G}`
`{SUm}`  | `{swap_used;M}`
`{SUp}`  | `{swap_used_percents}`
`{SUpi}` | `{swap_used_percents:1}`

### Deprecation Warnings

* Disk Space block: the `alias` has been deprecated in favour of using `format` and may be removed in a future release.

### New Blocks and Features

* Backlight block: new `invert_icons` config option for people using coloured icons (#1098 by @MaxVerevkin)
* Net block: new `format_alt` option to set an alternative format string to switch between when the block is clicked (#1063 by @MaxVerevkin)
* Nvidia block: new "Power Draw" option (#1154 by @quintenpalmer)
* Sound block: new `{output_description}` format key to show the PulseAudio device description
* Speedtest block: new `format` configuration option to customize the output of the block.
* Temperature block: add fallback for older systems without JSON support (#1070 by @ammgws)
* Weather block: new config option to set display language, and new format key `{weather_verbose}` to display textual verbose description of the weather, e.g. "overcast clouds" (#1169 by @halfcrazy)
* SIGUSR2 signal can now be used to reload i3status-rust in-place without restarting i3/swaybar (#1131 by @MaxVerevkin)
* New compile time feature `debug_borders` for debugging spacing issues (#1083 by @MaxVerevkin)
* New "material-nf" icon set (#1095 by @MaxVerevkin)
* New `icons_format` config option for overriding icon formatting on a per-block basis (#1095 by @MaxVerevkin)
 
### Bug Fixes and Improvements

* Music block: fix `on_collapsed_click` which was broken in a previous release (#1061 by @MaxVerevkin)
* Net block: print "N/A" when trying to get ssid or signal strength using wired connections instead of erroring out (#1068 by @MaxVerevkin)
* Networkmanager block: avoid duplicate device with VPN connections (#1099 by @ravomavain), fix cases where connections would not update (#1119 by TilCreator)
* Sound block: fix spacing for empty format strings (#1071 by @ammgws)

# i3status-rust 0.14.7

Bug fix release for compile error on 32bit systems

# i3status-rust 0.14.6

Fixes bug with loading config from file introduced in 0.14.4 (and also present in 0.14.5)

# i3status-rust 0.14.5

Fixes crash on i3 introduced in 0.14.4

# i3status-rust 0.14.4

### General Notices

* Due to a bugfix in the CPU block, when using the `{frequency}` and `{utilization}` format key specifiers,  "GHz" and "%" will be appended within the format keys themselves so there is no need to write them in your `format` string anymore.

### Deprecation Warnings

* Battery block config option `show` has been deprecated in favour of `format` (deprecated since at least v0.10.0 released in July 2019)

* Battery block config option `upower` has been deprecated in favour of `device` (deprecated since at least v0.10.0 released in July 2019)

* CPU Utilization block config option `frequency` has been deprecated in favour of `format` (deprecated since at least v0.10.0 released in July 2019)

* Network block config options `ssid`,  `signal_strength`, `bitrate`, `ip`, `ipv6`, `speed_up`, `speed_down`, `graph_up`, `graph_down` have been deprecated in favour of `format` (deprecated since v0.14.2 released in October 2020)

* Pacman block format key `{count}` has been deprecated in favour of `{pacman}` (deprecated since v0.14.0 released in June 2020)

* Taskwarrior block config option `filter_tags` has been deprecated in favour of `filters` (since v0.14.4 - this release)

### New Blocks and Features

* `on_click` option is now available for all blocks  (#1006 by @edwin0cheng)

* Github block: new option to hide block when there are no notifications (#1023 by @ammgws)

* Hueshift block: add support for gammastep (#1027 by @MaxVerevkin)

* Pacman block: new option to hide block when up to date (#982 by @ammgws)

* Taskwarrior block: support multiple filters with new `filters` option (#1008 by @matt-snider)

### Bug Fixes and Improvements

* Fix config error when using custom themes (#968 by @ammgws)

* Fix microphone icons in awesome5 (#1017 by @MaxVerevkin)

* Make blocks using http more resilient (#1011 by @simao)

* Various performance improvements/optimisations (#1033, #1039 by @MaxVerevkin)

* Bluetooth: monitor device availability to avoid erroring out block (#986 by @ammgws)

* CPU block: fix "{frequency}" format in per-core mode (#1031 by @MaxVerevkin)

* KDEConnect block: support new version of kdeconnect (v20.12.* and above)

* KeyboardLayout block: support both `{variant}` and `{layout}` when using the sway driver (#1028 by @MaxVerevkin)

* Music block: handle case when metadata is unavailable (#967 by @ammgws), add workaround for `playerctl` (#973 by @ammgws), various other bugfixes (see #972)

* Net block: fix overflow panic (#993 by @ammgws), better autodiscovery (#994 by @ammgws), fix issues with parsing JSON output (#998 by @ammgws), `speed_min_unit` is now correctly handled (#1021 by @MaxVerevkin), allow Unicode SSIDs to be displayed correctly (#995 by @2m)

* Speedtest block: use `speed_digits` to format ping as well (#975 by @GladOSkar), `speed_min_unit` is now correctly handled (#1021 by @MaxVerevkin)

* Xrandr block: do not leave zombie processes around (#990 by @ammgws)

# i3status-rust 0.14.3

### New Blocks and Features

* New Apt block for keeping tabs on pending updates on Debian based systems (#943 by @ammgws)

* New Notify block for controlling/monitoring your notification daemon's do-not-disturb status

* KeyboardLayout block: add `variant` format specifier for localebus (#940 by @ammgws)

* Music block: implement format string (#949 by @ammgws), allow right click to cycle between available players (#930 by @ammgws)

* Implement per-block colour overrides (#947 by @ammgws)

* New "native" and "semi-native" themes (#938 by @GladOSkar)

### Bug Fixes and Improvements

* Add git commit hash to version output (#915 by @ammgws)

* Replace `uuid` dependency with just `getrandom` (#921 by @ammgws)

* Fix alternating tint behaviour (#924 by @ammgws, #927 by @GladOSkar)

* Fix panic when no icon exists for Diskspace, KDEConnect blocks (#908, #910 by @ammgws)

* Fix spacing for Battery, Sound & NetworkManager blocks (#923 from @Stunkymonkey)

* Battery block: clamp 'time remaining' values to something more realistic (#912 by @ammgws)

* KeyboardLayout block: fix crash on sway (#918 by @gdamjan, #939 by @ammgws)

* Music block: completely overhaul update mechanism (#906 by @ammgws)

* Net block: do not error out when arrays are empty (#926 by @ammgws)

* Xrandr block: remove hardcoded icons (#911 by @ammgws)

# i3status-rust 0.14.2

### New Blocks and Features

* New Hueshift block (#802 by @AkechiShiro)

* Backlight block: add nonlinear brightness control via new `root_scaling` option (#882 by @dancek)

* Battery block: add `allow_missing_battery` option (#835 by @Nukesor)

* Bluetooth block: add `hide_disconnected` option to hide block when device is disconnected (#858 by @ammgws)

* CPU block: add `on_click` option (#813 by @Dieterbe)

* Custom block: add signal support (#822 by @Gelox), add `hide_when_empty` option to hide block when output is empty (#860 by @ammgws), add `shell` option to set the shell used (#861 by @ammgws)

* CustomDBus block: allow setting the icon and state (#757 by @jmgrosen)

* Disk Space block: add `format` string option (#714 by @jamesmcm)

* IBus block: add `format` string option (#765 by @ammgws)

* Music block: add `dynamic_width`option (#787 by @UnkwUsr), add `on_click` (#817 by @Dieterbe),  add `hide_when_empty` option (#892 by @ammgws), add `interface_name_exclude` option (#888 by @ammgws)

* Net block: add `format` string option (#738 by @gurditsbedi)

* NetworkManager block: add regex filters for interface names (#781 by @omertuc)

* Sound block: add support for input devices (#740 by @remi-dupre), and new `max_vol` config option (#796 by @ammgws)

* Temperature block: add `inputs` whitelist (#811 by @arraypad), add `scale` option (#895 by @rjframe)

* Time block: add `locale` option (#863 by @ammgws)

### Bug Fixes and Improvements

* Fix spacing for inline widgets (#866 from @DCsunset)

* Fix spacing for plain theme (#894 by @Stunkymonkey)

* Battery block: add `full_format` to show text when battery is full (#785 by @DCsunset)

* Custom block: ensure `command` and `cycle` are actually mutually exclusive (#899 by @ammgws)

* Focusedwindow block: fix panic under sway (#792, #793 by @ammgws)

* IBus block: fix logic for finding dbus address (#759 by @ammgws)

* KDEConnect block: fix panic (#743 by @v0idifier)

* Load block: fix cpu count (#859 by @ammgws)

* Music block: only respond to left clicks (#862 by @ammgws), allow scrolling to seek forward/backward (#873 by @ammgws)

* Net block: sed awk grep removal (#758 by @themadprofessor, #825 by @hlmtre), fix regex parsing (#821 by @Dieterbe), fix logic for `hide_inactive`/`hide_missing` (#897 by @GladOSkar)

* NVidia block: fix panics (#771 by @themadprofessor, #807, #846 by @ammgws)

* Pacman block: fix regex logic (#804 by @PicoJr)

* TaskWarrior block: don't count deleted items (#788 by @HPrivakos)

# i3status-rust 0.14.1

* Forgot to regenerate Cargo.lock when 0.14.0 was released

(No features/code changes from 0.14.0)

# i3status-rust 0.14.0

### New Blocks and Features

* New KDEConnect block (#717 by @ammgws)

* New CustomDBus block (#687 by @ammgws)

* New Network Manager block (#641 by @kennylevinsen). This block existed previously but was undocumented until it was overhauled completely by @kennylevinsen)

* New Taskwarrior block (#600 by @flying7eleven)

* New GitHub block (#425 by @jlevesy)

* Keyboard Layout block now supports `sway` (#670 by @ammgws), and also has a new `format` config option (#593 by @thiagokokada)

* IBus block now allows mapping of displayed engine to user configured value (#576 by @ammgws)

* Weather block now supports `humidity` and `apparent` (Australian Apparent Temperature) format specifiers (#640 by @ryanswilson59, @ammgws). Location can now also be set by name rather than ID using the new `place` option (#635 by @ammgws). Alternatively, the location can be guessed from your current IP address (#690 by @ammgws)

* Focused Window block new `show_marks` option to show marks instead of title (#532 by @ammgws)

* Net and Speedtest blocks now take `speed_min_unit` and `speed_digits` parameters to format speeds (#704, #707 by @GladOSkar, @ammgws). 

* Net block `ssid` config option now supports `iwctl` and `wpa_cli` (#625, #721 by @ammgws). Can now show bitrate for wired devices (#612 by @ammgws). New `ipv6` option (#647 by @ammgws)

* Pacman block now supports a `critical_updates_regex` parameter to control block state (#613 by @PicoJr), and now supports AUR as well (#658 by @PicoJr)

* Music block has a new `smart_trim` config option (#654 by @jgbyrne). Artist/title separator can now be customised with the `separator` option (#655 by @ammgws)

* Sound block now supports a `format` parameter (#618 by @jedahan). Along with that a format qualifier `output_name` was added which will show the name of the sink whose volume is being reported (#712 by @ammgws). ALSA driver: new `device` and `natural_mapping` options (#622 by @ammgws)

* CPU block now has `per_core` support for `{frequency}`, `{utilization}` (@grim7reaper)

* Block `interval` config can now take `"once"` in order to run blocks only one time (#684 by @PicoJr)

* Update font awesome icons to version 5 (#619 by @carloabelli)

* Add support for progress bars to some blocks (#578 by @carloabelli)

* Themes can now be read from standalone files (#611 by @atheriel & @PicoJr)

* New command line option `--never-pause` which will ignore any attempts by i3 to pause the bar when hidden/full-screen (#701 by @ammgws)

* If no config file path is supplied then we default to XDG_CONFIG_HOME/i3status-rust

### Bug Fixes and Improvements

* Net block fixed to support ppp vpn (#570 by @MiniGod). Device is now auto selected by default (#626 by @ammgws). Fixed error in `use_bits` calculation (#704 by @ammgws). Use /sys/class/net/<device>/carrier instead of operstate in is_up() (#605 by @happycoder97, @ammgws)

* Music block artist parsing from metadata fixed (#561 by @Riey)

* Fix panics for blocks without update intervals (#582 by @ammgws)

* Nvidia block: make threshold configurable, swap idle/good (#615 by @ammgws). Also fixed utilisation to have a fixed width (#566 by @TheJP)

* Backlight block now reads from actual_brightness as per kernel docs (#631 by @ammgws), with a special case for amdgpu backlights which don't follow the spec (#643 by @ammgws)

* Battery block now has a fallback for determining power consumption (#653 by @ammgws), and the time remaining is now only displayed when useful (#591 by @debugloop)

* Time block fixed to only register left mouse button clicks (#628 by @ammgws)

* Toggle block fixed to only toggle if command exited successfully (#648 by @ammgws)

* Fix missing icons for `bat_half` in the none theme (#719 by @varunkashyap) 

* Fix panic in CPU block if >32 CPUs present (#639 @snicket2100)

* Fix panic in Memory block when wrong placeholder given (#616 by @ammgws)

* Fix missing `good_bg` and `good_fg` theme overrides (#630 by @carloabelli)

* Unified handling of stdin and stdout to prevent broken pipe errors (#594 by @Celti)

* Travis CI will now run clippy for all features and targets (#682 by @rotty)

* Dependent crates have been updated to their latest versions (#729 by @ammgws, @ignatenkobrain)

### Documentation

* Document `info`, `good`, `warning`, `critical` parameters for the Battery block (@ammgws)

* Document `interval` for Notmuch, Uptime blocks (@ammgws)

* Fix error in Pomodoro block docs (#646 by @kAworu)

* Add profiling.md (#649 by @PicoJr)

* Adds a man page #556

# i3status-rust 0.13.1

* Dependent crates have been updated to their latest versions to make downstream
  packaging easier. This will become part of the normal release process in the
  future. (#540 by @rotty, #551 by @atheriel)

# i3status-rust 0.13.0

### New Blocks and Features

* The Net block now takes a `use_bits` parameter to display speeds in bit-based
  instead of byte-based units. (#496 by @hlmtre)

* The Pacman block now supports a `format` parameter. (#473 by @ifreund)

* The top-level config now takes a `scrolling` parameter that can be used to
  turn on `"natural"` mouse scrolling in the bar. (#494 by @bakhtiyarneyman)

* The Brightness block will now fall back to using D-Bus for changing the
  brightness if it cannot modify it via `sysfs`. (#499 by @majewsky)

* The Bluetooth block now allows for setting a text `label` parameter to keep
  track of devices. (#528 by @jeffw387)

### Bug Fixes and Improvements

* Fixes a panic that could sometimes manifest when restarting Pulseaudio. (#484
  by @ammgws)

* Fixes errors in the Pango markup we generate. (#518 by @ammgws)

* Fixes a potential panic when the Focused Window block was the only one in the
  configuration. (#535 by @ammgws)

* Fixes potential issues due to not ignoring `stdin` and `stdout` when spawning
  child processes. (#530 by @Celti)

* Improvements to the spacing around icons and IP addresses in the Net block.
  (#505 and #507 by @ammgws)

* Bumps several dependencies to fix security issues and reduce the number of
  transitive dependencies, which should improve build times. (#491, #492, #493,
  #510, #523 by @ammgws)

* Updates the installation documentation for Fedora. The project is now in the
  official repos! (#488 by @tim77)

* Simplifies the `udev` rule in the Brightness block docs. (#481 by @hellow554)

* Fixes a typo in the theme documentation. (#485 by @peeweep)

* Adds mention in the documentation that the Focused Window block is compatible
  with Sway. (#497 by @NilsIrl)

* Adds documentation for the optional Notmuch mail block. (#527 by @ammgws)

* Travis CI will now compile the project with all features enabled, which would
  have caught several bugs long ago. (#539 by @rotty)

# i3status-rust 0.12.0

### New Blocks and Features

* Wireguard devices are now correctly identified as VPNs in the net block. (#419
  by @vvrein)

* The keyboard layout block now has a `kbddbus` driver. (#451 by @sashomasho)

* Adds a new Pomodoro block. (#453 by @ghedamat)

### Bug Fixes and Improvements

* Fixes a panic in the iBus block due to the use of Perl regex features. (#443
  by @ammgws)

* Fixes more 32-bit build issues (e.g. for armv6 and i686). (#449 and #450 by
  @jcgruenhage)

* We now enforce `cargo fmt` on the codebase and in Travis CI. (#457 by
  @atheriel and @kennylevinsen, #474 by @ifreund)

* Improves parsing of `setxkbmap` output. (#458 by @sashomasho)

* Improvements to character width calculations in the rotating text widget.
  (#437 by @ammgws)

* Adds Fedora, NixOS, and Void Linux installation info to the `README`. (@tim77
  and @atheriel)

* The Font Awesome icons now use `bat_quarter` and `bat_three_quarters` for
  battery ranges. (#393 by @Ma27)

* Adds documentation for `hide_missing` and `hide_inactive` in the net block.
  (#476 by @bascht)

# i3status-rust 0.11.0

### New Blocks and Features

* Adds a new Docker block, which can display information about containers
  overseen by the Docker daemon. (#413 by @jlevesy)

* Adds a new Notmuch block for querying information from a Notmuch mail
  database. This block is currently an optional feature and must be enabled with
  `cargo build --features notmuch`. (#215 by @bobthemighty and @atheriel)

* The Weather block will now obey the `OPENWEATHERMAP_API_KEY` and
  `OPENWEATHERMAP_CITY_ID` environment variables. (#410 by @nicholasfagan)

* The Net block can now display wifi signal strength. (#418 by @bnjbvr)

* The project now has improved crate metadata, a proper `CONTRIBUTING.md` file,
  and will put release notes in a `NEWS.md` file. (by @atheriel)

### Bug Fixes and Improvements

* Updates the `nix` crate to fix broken builds on aarch64 with musl libc (#402).

* Fixes builds on i686. (#406 by @Gottox)

* Fixes a potential crash due to missing wind speed or direction in the Weather
  block. (#407 by @bramvdbogaerde).

* Fixes omission of UPower batteries that do not have a `battery_` prefix. (#423
  by @freswa)

* Fixes our use of now-deprecated dynamic trait and range syntax language
  features. (#428 by @duac)

* Prunes some transient dependencies. (#434 by @ohk2kt3t4 and @ammgws)

* Fixes our use of a deprecated flag in our `rustfmt` configuration. (#438 by
  @ammgws)

* Internal refactoring to reduce merge conflicts when adding new blocks. (by
  @atheriel)

# i3status-rust 0.10.0

* First tagged release.
