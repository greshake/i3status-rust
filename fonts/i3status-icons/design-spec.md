# Icon Set Specification — i3status-rust status bar

i3status-rust draws a thin horizontal strip along the edge of a computer screen. The strip is divided into small "blocks", each showing one piece of live information: battery level, network status, volume, the time, the weather, and so on. Almost every block begins with a small icon that tells the user at a glance *what* the number or word next to it means. You are designing the complete replacement set of these icons — 77 named icons in total, several of which are series of ordered states rather than single images.

## Design constraints

- **Tiny rendering size.** Each icon appears at roughly the height of a line of text, inside a slim horizontal bar. Design at that size, not for a poster: whatever doesn't survive being shrunk to a lowercase letter's height must go.
- **Monochrome only.** Every icon is a single flat color; the bar itself decides the tint (and may recolor icons to signal warnings). No gradients, no second color, no reliance on color to carry meaning.
- **Instant legibility.** Each icon must be identifiable in a split-second sideways glance. Prefer bold, unmistakable silhouettes over interior detail.
- **One family.** All 77 icons must feel like siblings: the same stroke weight, the same corner treatment, the same visual density, the same optical size. A user will see a dozen of them side by side all day.
- **Icons stand alone.** Each icon sits next to a number or a short word, with no label explaining it. The silhouette carries the entire meaning, so clarity beats cleverness and detail.
- **Progressions must self-order.** Several icons come as a series of 3–6 states showing "how much" (battery fill, signal strength, volume, temperature, brightness). Any single state must be readable on its own — the user never sees the other steps for comparison. Use fill level, number of bars/arcs, or another visibly ordered device where "more ink = more of the thing".
- **Avoid twins.** Several icons in the current set accidentally share the same image (noted below). In the new set, no two different meanings may use an identical or near-identical drawing.

---

## Power & battery

### `bat` — battery level (series of 5)
- **Represents:** How full the laptop's (or a connected phone's) battery is, from nearly empty to full.
- **Current icon:** A horizontal battery outline with a small terminal nub, shown in five fill states from empty to full. The plain-text fallback is "BAT".
- **Should look like:** Keep the metaphor — a battery outline whose interior fills progressively in 5 ordered steps (empty, quarter, half, three-quarters, full). Low = almost no ink inside (danger of running out); high = solid fill (fully charged). The battery shape is universally understood and the fill level is self-ordering.

### `bat_charging`
- **Represents:** The battery is currently plugged in and charging.
- **Current icon:** A wall plug (two-prong power plug); the emoji set uses 🔌.
- **Should look like:** A battery outline with a lightning bolt inside or overlaid — this keeps it in the same visual family as `bat` while the bolt unambiguously says "power is flowing in". A bare wall plug reads as "electricity" generally, not specifically "battery charging".

### `bat_not_available`
- **Represents:** No battery information can be read (e.g. a desktop machine, or the battery was removed).
- **Current icon:** The empty-battery outline (same drawing as the lowest `bat` state) — confusingly identical to "battery almost dead".
- **Should look like:** A battery outline with a slash through it, or with a question mark inside. It must be clearly distinct from "empty battery": the current twin is a genuine problem, since "no battery" and "battery about to die" mean very different things.

---

## Audio

### `volume` — speaker loudness (series of 3)
- **Represents:** The current output volume of the computer's speakers, from quiet to loud.
- **Current icon:** A speaker cone seen from the side, with 0, 1, or 2 sound waves radiating from it as volume rises.
- **Should look like:** Keep the metaphor — the same speaker silhouette with an increasing number of sound-wave arcs (none/one/two, or one/two/three). Low = bare speaker (quiet); high = speaker with full arcs (loud). More arcs = more sound is instantly readable.

### `volume_muted`
- **Represents:** Sound output is muted.
- **Current icon:** A speaker with an X (or slash) beside it.
- **Should look like:** Keep the metaphor — the same speaker silhouette as `volume` with a clear diagonal slash or a small × where the waves would be. It must share its base shape with `volume` so the pair reads as on/off versions of the same thing.

### `microphone`
- **Represents:** The microphone is active (also used by the privacy monitor to warn that some app is listening).
- **Current icon:** A studio-style microphone on a slight diagonal.
- **Should look like:** Keep the metaphor — a simple handheld/studio microphone silhouette (rounded capsule head on a short stem). It's the universal "mic" mark. Because this icon can be a privacy warning, its silhouette must be very bold and unmistakable.

### `microphone_muted`
- **Represents:** The microphone is muted / not capturing.
- **Current icon:** The same microphone with a diagonal slash through it.
- **Should look like:** Keep the metaphor — identical microphone to `microphone` with a clean diagonal slash. The slash treatment should match the one used on `volume_muted` and `bell-slash` so "slash = off" is one consistent rule across the family.

### `headphones`
- **Represents:** A pair of Bluetooth headphones is connected (shown by the Bluetooth block, usually next to their battery level).
- **Current icon:** Over-ear headphones (headband arc with two ear cups).
- **Should look like:** Keep the metaphor — a headband arc ending in two solid ear cups. It's a strong, symmetric silhouette that survives tiny sizes well.

---

## Networking

### `net_wireless` — Wi-Fi (series of signal states)
- **Represents:** The computer is connected via Wi-Fi; ideally the icon also conveys how strong the signal is.
- **Current icon:** A single classic Wi-Fi fan (a dot with radiating arcs) with no strength variation.
- **Should look like:** The Wi-Fi fan — a dot at the bottom with 3 concentric arcs above — drawn as an ordered series: low = only the dot (or dot plus one faint/hollow arc), high = dot plus all arcs solid. Low means barely any signal; high means full strength. Keep the universally recognized fan shape; the arc count self-orders the series. (A single full-fan version should also work standing alone, since some uses show only one state.)

### `net_cellular` — mobile signal (series of 6)
- **Represents:** Signal strength of a connected phone or cellular modem, from no signal at all up to full bars.
- **Current icon:** The classic ascending signal-bars mark (a staircase of vertical bars); the plain-text fallback spells out "NO SIGNAL", "0 BARS" … "4 BARS".
- **Should look like:** Keep the metaphor — a staircase of 4 ascending bars in six states: state 1 = no bars at all plus a slash or × (no signal), state 2 = all bars hollow/ghosted (connected, zero strength), states 3–6 = one, two, three, four bars filled. Low = little or no ink (weak/no signal); high = full solid staircase (strong signal). Filled-vs-hollow bars keep every state readable alone.

### `net_wired`
- **Represents:** The computer is connected to the network by an Ethernet cable.
- **Current icon:** An Ethernet port/connector (the rectangular plug with pins).
- **Should look like:** Keep the metaphor — the rectangular Ethernet plug seen face-on (a squarish outline with a notched underside and a few pin teeth), simplified to its chunkiest recognizable form. It's the standard mark for wired networking.

### `net_vpn`
- **Represents:** Traffic is going through a VPN — an encrypted, private connection.
- **Current icon:** A padlock.
- **Should look like:** Keep the metaphor — a closed padlock (rounded shackle over a solid body). The padlock is the clearest tiny-size symbol for "your connection is protected". A shield would also work if the padlock is needed elsewhere, but the set currently has no conflict.

### `net_bridge`
- **Represents:** A network bridge — a virtual interface that joins two networks together (a technical setup, common on machines running virtual machines).
- **Current icon:** A "sitemap" hierarchy diagram: one box on top connected down to three boxes.
- **Should look like:** Two nodes (dots or small squares) joined by a horizontal link — literally two things bridged together. The current three-way tree reads as "org chart" and says nothing about joining. Alternatively an actual tiny bridge silhouette (deck with two towers), matching the 🌉 emoji fallback, if it stays legible at text height.

### `net_modem`
- **Represents:** A dial-up or mobile-broadband modem connection.
- **Current icon:** An old-fashioned telephone handset — a nod to dial-up.
- **Should look like:** Keep the metaphor if you like its retro charm (a curved telephone handset is a fine mark for "dial-up era connection"); otherwise a small box with a short antenna (a classic modem/router silhouette) is a more literal option. Either way it must not be confusable with `phone` (a modern smartphone rectangle).

### `net_loopback`
- **Represents:** The loopback interface — the computer talking to itself (a technical, local-only network).
- **Current icon:** None — it currently just shows the letters "LO".
- **Should look like:** An arrow that loops back on itself into a closed circle (a single circular arrow returning to its own tail). "A loop that comes back to where it started" is exactly what loopback means. This one is a brand-new drawing with no precedent in the set.

### `net_up`
- **Represents:** Upload — data being sent from the computer (upload speed in the network and speed-test blocks).
- **Current icon:** An upload mark: an arrow rising out of a tray/box.
- **Should look like:** Keep the metaphor, simplified: a bold upward arrow (optionally over a short baseline). It will usually sit right next to `net_down`, so the two must be exact mirror twins differing only in direction.

### `net_down`
- **Represents:** Download — data being received (download speed in the network and speed-test blocks).
- **Current icon:** A download mark: an arrow dropping into a tray/box.
- **Should look like:** Keep the metaphor: the exact mirror of `net_up` — a bold downward arrow (optionally into a short baseline). Note this same icon is also used to mean "network is down / disconnected" in the VPN block, so avoid making it look error-like; it is simply "incoming".

### `ping`
- **Represents:** Ping — the round-trip time of the connection, in milliseconds (how "snappy" the network feels).
- **Current icon:** Two horizontal arrows pointing opposite ways (an "exchange" mark); the emoji set jokes with a ping-pong paddle 🏓.
- **Should look like:** Keep the round-trip idea but make it clearly cyclical: two small horizontal arrows stacked, top pointing right and bottom pointing left (out-and-back). A stopwatch is a tempting alternative but would collide with the timer icons; the out-and-back arrows say "signal goes there and comes back".

### `bluetooth`
- **Represents:** A Bluetooth device is connected (the block shows the device and often its battery).
- **Current icon:** The standard Bluetooth rune (the angular ᛒ-like mark).
- **Should look like:** Keep the metaphor — the Bluetooth rune, drawn in the set's stroke weight. It is a trademark-grade symbol everyone recognizes; inventing a substitute would only confuse.

---

## Weather

The weather block shows current conditions next to the temperature. Day and night variants of the same condition exist; night versions should swap the sun for a crescent moon but otherwise keep the identical cloud/rain drawing, so the pairs are obviously related.

### `weather_sun`
- **Represents:** Clear, sunny daytime weather.
- **Current icon:** A sun — circle with radiating rays.
- **Should look like:** Keep the metaphor — a circle with 8 short stubby rays. The most legible tiny sun has few, thick rays.

### `weather_moon`
- **Represents:** Clear night sky.
- **Current icon:** A crescent moon.
- **Should look like:** Keep the metaphor — a simple crescent, matching the moon used in the night variants below so "crescent = night" is one consistent rule.

### `weather_clouds`
- **Represents:** Cloudy or overcast daytime.
- **Current icon:** A single puffy cloud.
- **Should look like:** Keep the metaphor — one solid puffy cloud silhouette (flat base, two or three bumps). This cloud becomes the base shape for rain, snow, fog and thunder, so design it first.

### `weather_clouds_night`
- **Represents:** Cloudy night.
- **Current icon:** A cloud with a small moon peeking behind it.
- **Should look like:** Keep the metaphor — the base cloud with a small crescent tucked behind its upper corner.

### `weather_rain`
- **Represents:** Rain during the day.
- **Current icon:** A cloud with sun and rain streaks (a busy three-element drawing).
- **Should look like:** The base cloud with 2–3 short falling drops or slanted dashes beneath it. Drop the sun — at text height, cloud+sun+rain is too much; cloud-with-drops is the universal rain mark.

### `weather_rain_night`
- **Represents:** Rain at night.
- **Current icon:** A cloud with a moon and rain streaks.
- **Should look like:** The rain cloud above with the small crescent tucked behind it — same drops, same cloud, moon added.

### `weather_snow`
- **Represents:** Snowfall.
- **Current icon:** A single snowflake.
- **Should look like:** Keep the metaphor — a six-armed snowflake with thick, simple arms (no delicate branching, which vanishes at small sizes). A cloud with tiny flakes also works if you prefer every precipitation icon to share the cloud base.

### `weather_thunder`
- **Represents:** Thunderstorm during the day.
- **Current icon:** A bare lightning bolt.
- **Should look like:** The base cloud with a bold lightning bolt dropping from it. A bare bolt reads as "electricity/energy" (and risks confusion with battery charging); cloud-plus-bolt reads unmistakably as storm.

### `weather_thunder_night`
- **Represents:** Thunderstorm at night.
- **Current icon:** The same bare lightning bolt (no night variant at all today).
- **Should look like:** The storm cloud above with the small crescent behind it — giving night storms the distinct variant they currently lack.

### `weather_fog`
- **Represents:** Fog or mist during the day.
- **Current icon:** Just the generic cloud (no distinct fog drawing); the emoji set shows a fog-covered bridge 🌁.
- **Should look like:** Two or three horizontal wavy or straight lines, optionally under a low cloud — the standard "fog banks" mark. It needs to be clearly different from plain clouds, which it currently is not.

### `weather_fog_night`
- **Represents:** Fog at night.
- **Current icon:** The same generic cloud again.
- **Should look like:** The fog lines with the small crescent above them.

### `weather_default`
- **Represents:** Weather conditions unknown or not reported — the fallback.
- **Current icon:** The plain cloud again (so it is indistinguishable from "cloudy").
- **Should look like:** A cloud with a small question mark inside or beside it. The fallback must be visually distinct from genuine cloudy weather.

---

## Music & media

### `music`
- **Represents:** The music player block — current song title and artist.
- **Current icon:** A musical note (beamed note).
- **Should look like:** Keep the metaphor — a single eighth-note or a beamed pair of notes, with solid note-heads. Nothing says "music" more directly.

### `music_play`
- **Represents:** A clickable play button (starts playback; also indicates paused state waiting to play).
- **Current icon:** The standard right-pointing solid triangle.
- **Should look like:** Keep the metaphor — solid right-pointing triangle. These four transport marks are a universal, untouchable vocabulary; just redraw them in the family's weight and optical size.

### `music_pause`
- **Represents:** A clickable pause button (playback is currently running).
- **Current icon:** Two vertical bars.
- **Should look like:** Keep the metaphor — two solid vertical bars, matching the play triangle's optical weight.

### `music_next`
- **Represents:** A clickable skip-to-next-track button.
- **Current icon:** A solid triangle pointing right against a vertical bar ("step forward").
- **Should look like:** Keep the metaphor — triangle-plus-bar pointing right (or double triangle), mirror of `music_prev`.

### `music_prev`
- **Represents:** A clickable skip-to-previous-track button.
- **Current icon:** A solid triangle pointing left against a vertical bar.
- **Should look like:** Keep the metaphor — the exact mirror of `music_next`.

---

## Time & productivity

### `time`
- **Represents:** The clock block — the current time and date.
- **Current icon:** A round analog clock face with hands.
- **Should look like:** Keep the metaphor — a circle with two hands (e.g. at ten-past-ten for pleasing symmetry). The definitive "time" mark.

### `calendar`
- **Represents:** The calendar block — the user's next upcoming appointment.
- **Current icon:** A wall calendar page (rectangle with two binder rings on top and a grid).
- **Should look like:** Keep the metaphor — calendar page with the two top tabs; at tiny sizes drop the inner grid to at most a couple of dots, or mark a single "day". Distinct from all other rectangles in the set thanks to the top tabs.

### `uptime`
- **Represents:** How long the computer has been running since it was last started.
- **Current icon:** An hourglass, half empty.
- **Should look like:** Keep the metaphor — a simple hourglass (two triangles meeting at a waist). It says "elapsed time" and is nicely distinct from the round clock used for `time`.

### `pomodoro`
- **Represents:** The pomodoro productivity timer block (work in timed 25-minute sprints; the technique is named after a tomato-shaped kitchen timer).
- **Current icon:** A tomato emoji 🍅.
- **Should look like:** Keep the metaphor — a round tomato silhouette with a small leafy stem on top. It's quirky but it is *the* symbol of this technique and its fans will look for it.

### `pomodoro_started`
- **Represents:** A work sprint is currently running.
- **Current icon:** The standard play triangle (identical to `music_play`).
- **Should look like:** Keep the play-triangle language but differentiate from the music transport controls — e.g. a play triangle inside a circle outline. Same rule for the three siblings below, so the pomodoro states form their own consistent sub-family.

### `pomodoro_paused`
- **Represents:** The sprint timer is paused.
- **Current icon:** Two pause bars (identical to `music_pause`).
- **Should look like:** Pause bars inside a circle outline, matching `pomodoro_started`.

### `pomodoro_stopped`
- **Represents:** The timer is stopped / not in use.
- **Current icon:** A solid square (standard stop mark).
- **Should look like:** A stop square inside a circle outline, matching the sub-family.

### `pomodoro_break`
- **Represents:** Break time between work sprints.
- **Current icon:** A steaming coffee cup — currently the *same drawing* as the tea timer's icon.
- **Should look like:** A coffee cup is the right idea for "take a break", but it must be visibly different from `tea` below — e.g. a small espresso cup with a saucer here, versus a mug or teapot for `tea`. Two identical cups meaning different blocks is a real conflict today.

### `tea`
- **Represents:** The tea timer block — a countdown for steeping tea.
- **Current icon:** The same steaming coffee cup as `pomodoro_break`.
- **Should look like:** A mug with a teabag string and tag hanging over the rim, or a small teapot. The teabag string is the detail that says "tea, steeping" and breaks the twin-cup conflict.

### `tasks`
- **Represents:** The to-do block — the number of pending tasks in the user's task list.
- **Current icon:** A list with horizontal progress bars; the emoji set uses a checkmark ✅.
- **Should look like:** A checklist: two or three stacked lines with a checkmark (or check-box) at the start of the first line. A checklist reads instantly as "things to do" next to a count.

### `mail`
- **Represents:** Unread email — shown next to the count of new messages.
- **Current icon:** A closed envelope.
- **Should look like:** Keep the metaphor — a closed envelope (rectangle with a V-flap). The strongest possible "mail" silhouette.

### `github`
- **Represents:** The number of unread notifications on GitHub (a website where programmers collaborate; its mascot is a cat-octopus creature).
- **Current icon:** The official GitHub logo — the round "octocat" silhouette.
- **Should look like:** Keep the metaphor — a round-badge cat silhouette in the style of the GitHub mark, redrawn to the family's weight. Users identify the service by this mark; anything else would be unrecognizable. (Stay close-but-not-identical to the trademark as your practice requires.)

### `bell`
- **Represents:** Notifications are enabled / there are pending notifications (shown with a count).
- **Current icon:** A classic bell.
- **Should look like:** Keep the metaphor — a solid bell with a small clapper dot. The universal notification mark.

### `bell-slash`
- **Represents:** Notifications are silenced (do-not-disturb).
- **Current icon:** The same bell with a diagonal slash.
- **Should look like:** Keep the metaphor — identical bell with the family's standard diagonal slash, consistent with the other "-muted/off" icons.

### `notification`
- **Represents:** Notifications waiting on a connected phone (shown by the phone-companion block, next to a count).
- **Current icon:** A bell — the *same drawing* as `bell`, which is a conflict.
- **Should look like:** A bell with a small badge dot at its upper corner (like an app icon with an alert dot), or a small speech bubble with a dot. It must be related to, but distinguishable from, the plain `bell`, since both can appear in the same bar.

---

## Hardware & system

### `cpu`
- **Represents:** How hard the computer's processor is working (utilization percentage).
- **Current icon:** A speedometer/tachometer dial — an odd borrowed metaphor; the emoji set uses a robot 🤖.
- **Should look like:** A processor chip: a square with a grid of stubby pins protruding from all four sides. This is the literal, widely used mark for CPU and frees the speedometer metaphor entirely. (Note: this icon can render as a progression in some setups, but a single chip is the primary form.)

### `cpu_boost_on`
- **Represents:** The processor's "turbo boost" (temporary extra speed) is switched on.
- **Current icon:** A toggle switch in the "on" position — identical to `toggle_on`, a conflict; the emoji set charmingly uses a rabbit 🐇.
- **Should look like:** The CPU chip with a small lightning bolt or up-arrow at its corner, or a bolt alone in a circle. Tying it visually to the chip explains *what* is boosted, and it breaks the twin-toggle conflict.

### `cpu_boost_off`
- **Represents:** The processor's turbo boost is switched off.
- **Current icon:** A toggle switch in the "off" position — identical to `toggle_off`; emoji: a tortoise 🐢.
- **Should look like:** The same chip-with-bolt as `cpu_boost_on` with the family's diagonal slash across the bolt. On/off must read as a pair.

### `cogs`
- **Represents:** System load average — a general "how busy is this machine" number.
- **Current icon:** Two interlocking gears.
- **Should look like:** Keep the metaphor — two meshed gears (one larger, one smaller, overlapping). Gears say "machinery working" and are distinct from the chip.

### `memory_mem`
- **Represents:** How much of the computer's working memory (RAM) is in use.
- **Current icon:** A microchip (square chip with pins); emoji uses a thought bubble 💭.
- **Should look like:** A RAM stick: a wide, low rectangle with a row of small square chips inside and shallow teeth along the bottom edge. This keeps "memory = chip family" but stays clearly different from the square CPU chip.

### `memory_swap`
- **Represents:** Swap usage — overflow memory that spills onto the disk when RAM is full.
- **Current icon:** A hard-disk drive — the *same drawing* as `disk_drive`, a conflict.
- **Should look like:** The RAM-stick shape with two small opposing arrows (a swap/exchange mark) beside or inside it — "memory being traded back and forth". This links it to memory rather than to the disk block and removes the twin.

### `disk_drive`
- **Represents:** Disk usage or disk activity — free space left, or read/write speed.
- **Current icon:** A hard-disk drive (rounded rectangle with a dot/indicator).
- **Should look like:** Keep the metaphor — a drive: a horizontal rounded rectangle with a small indicator dot in one corner, or the classic cylinder ("database drum") if you prefer. Must remain visually distinct from the RAM stick.

### `gpu`
- **Represents:** Statistics of the graphics card (the chip that drives the screen and heavy visual work).
- **Current icon:** A television/monitor — the *same drawing* as `xrandr`, a conflict, and "monitor" is not "graphics card".
- **Should look like:** A graphics card silhouette: a horizontal board with a visible fan circle on it (rectangle plus one inset circle, maybe a bracket notch). The fan-on-a-board shape is how gamers everywhere draw a GPU, and it frees the monitor image for `xrandr`.

### `thermometer` — temperature (series of 5)
- **Represents:** The computer's internal temperature, from cool to worryingly hot.
- **Current icon:** A classic bulb thermometer in five fill states from empty to full.
- **Should look like:** Keep the metaphor — a vertical thermometer (thin tube with a round bulb at the bottom) whose mercury column rises in 5 ordered steps. Low = only the bulb filled (cool); high = column filled to the top (hot). The rising column is perfectly self-ordering.

### `backlight` — screen brightness (series of 5)
- **Represents:** How bright the screen's backlight is set, from dim to full brightness.
- **Current icon:** Moon phases, from full moon to new moon — and the order is inverted (the dimmest setting shows the brightest moon), which is genuinely confusing.
- **Should look like:** Replace with a sun/brightness mark: a circle whose rays grow in 5 steps — low = small dim disc with no or tiny rays (dim screen); high = full disc with long bold rays (bright screen). This is the brightness symbol used on every keyboard's brightness keys, and it self-orders in the correct direction.

### `hueshift`
- **Represents:** The screen's color temperature in Kelvin — the warm/cool tint of a "night light" filter.
- **Current icon:** A lightbulb.
- **Should look like:** A half-and-half circle: one half sun-rayed and one half moon-crescent, or a circle split by a wavy line — "the screen shifting between day-white and evening-warm". A bulb says "idea/light" but nothing about tint; the day/night split circle explains the feature. (A thermometer must be avoided — taken by temperature.)

### `keyboard`
- **Represents:** A connected (Bluetooth) keyboard, usually shown with its battery level.
- **Current icon:** A keyboard (wide rectangle full of tiny key squares).
- **Should look like:** Keep the metaphor — wide, low rectangle with a coarse hint of keys: a few fat dots and one wide space-bar line. Do not draw a full key grid; it turns to mush at text height.

### `mouse`
- **Represents:** A connected (Bluetooth) computer mouse, with its battery level.
- **Current icon:** An arrow cursor (the on-screen pointer) — odd, since the block is about the physical device.
- **Should look like:** The physical mouse: an upright rounded oval with a center line splitting two buttons and/or a small scroll-wheel dot. The device silhouette matches the neighboring keyboard/headphones/gamepad icons, which are all physical objects.

### `joystick`
- **Represents:** A connected game controller, with its battery level.
- **Current icon:** A gamepad (two-handled controller with d-pad and buttons).
- **Should look like:** Keep the metaphor — a gamepad silhouette with two rounded grips, a cross on the left and two button dots on the right. Instantly readable as "game controller".

### `xrandr`
- **Represents:** Information about the connected monitor/screen (name, brightness, resolution).
- **Current icon:** A television/monitor (shared with `gpu` today).
- **Should look like:** Keep the monitor metaphor and give it sole ownership: a screen on a small stand (rectangle plus stem and foot). Once `gpu` becomes a fan-board, this conflict disappears.

### `resolution`
- **Represents:** The monitor's pixel resolution (e.g. "1920×1080"), shown inside the screen-info block.
- **Current icon:** An empty square outline.
- **Should look like:** A screen rectangle with small corner brackets or arrows pointing outward from its corners — "the size/dimensions of the display area". A bare square says nothing; corner marks say "dimensions".

### `webcam`
- **Represents:** The camera is in use (a privacy warning that some app is watching), or camera status.
- **Current icon:** A video camera (camcorder body with lens cone).
- **Should look like:** Keep the metaphor — either the camcorder-with-lens-cone or a round webcam eye (circle within circle atop a small foot). As a privacy warning it must be bold and instantly alarming-at-a-glance; the circular "eye" form is very strong at tiny sizes.

### `phone`
- **Represents:** A phone is connected to the computer (companion app link — shows the phone's battery, signal and notifications).
- **Current icon:** A modern smartphone (tall rounded rectangle).
- **Should look like:** Keep the metaphor — a tall rounded rectangle with a tiny speaker slit or home-dot. The simplest possible smartphone.

### `phone_disconnected`
- **Represents:** The companion phone is not reachable / link is broken.
- **Current icon:** The "no mobile phones" emoji 📵 (phone with slash) — visually foreign to the rest of the set.
- **Should look like:** The same smartphone as `phone` with the family's standard diagonal slash. Pairing must be obvious at a glance.

### `docker`
- **Represents:** Docker status — the number of software "containers" running on the machine (Docker's mascot is a whale carrying shipping containers).
- **Current icon:** The Docker mark: a ship/whale carrying a stack of boxes — busy at small sizes.
- **Should look like:** Simplify toward the brand's essence: a whale silhouette with 2–3 simple rectangles stacked on its back, or just a small stack of shipping-container rectangles. The container/whale imagery is what the audience associates with Docker; a plain ship reads as "ferry".

### `update`
- **Represents:** Pending software updates — the count of packages waiting to be upgraded.
- **Current icon:** A bare upward arrow — easily confused with "upload".
- **Should look like:** An upward arrow inside a circle (or a box with an up-arrow rising from it) — "bring the system up to date". The enclosing circle separates it from the plain up/down transfer arrows in the networking group.

### `refresh`
- **Represents:** A clickable "restart this block" button that appears when one of the bar's widgets has crashed.
- **Current icon:** Two circular chasing arrows (the classic refresh/sync mark).
- **Should look like:** Keep the metaphor — one or two arrows bent into a circle, chasing each other. The universal reload symbol; keep the arrowheads chunky so they survive tiny sizes.

### `scratchpad`
- **Represents:** The window "scratchpad" — a hidden stash where users park windows off-screen; the icon sits next to a count of stashed windows.
- **Current icon:** Two overlapping window frames ("restore window" mark).
- **Should look like:** Keep the metaphor — two overlapping rounded rectangles, the back one peeking out behind the front one, suggesting "windows set aside in a pile". It reads well next to a count.

---

## Miscellaneous

### `toggle_on`
- **Represents:** A general-purpose on/off switch block, currently in the ON state (the user clicks it to flip something the user configured).
- **Current icon:** A pill-shaped toggle switch with the knob on the right.
- **Should look like:** Keep the metaphor — a pill outline with a solid round knob at the right end; make the ON state visually "filled" (solid pill, hollow knob or vice versa) so on/off differ by more than knob position alone (safer at tiny sizes and for quick glances).

### `toggle_off`
- **Represents:** The same switch block in the OFF state.
- **Current icon:** The toggle with the knob on the left.
- **Should look like:** The mirror of `toggle_on` with the "empty" treatment (hollow pill). The pair must read as two states of one object.

### `unknown`
- **Represents:** A fallback shown when the bar cannot identify something (for example an unrecognized device in the privacy monitor).
- **Current icon:** A question mark.
- **Should look like:** Keep the metaphor — a bold question mark, optionally inside a circle to give it the same visual weight as the pictorial icons. Honest and unambiguous.

---

## Quick checklist of conflicts to resolve (summary)

- `bat_not_available` vs. empty `bat` — currently identical.
- `memory_swap` vs. `disk_drive` — currently identical.
- `notification` vs. `bell` — currently identical.
- `tea` vs. `pomodoro_break` — currently identical.
- `gpu` vs. `xrandr` — currently identical.
- `cpu_boost_on/off` vs. `toggle_on/off` — currently identical.
- `weather_fog`, `weather_fog_night`, `weather_default` vs. `weather_clouds` — currently identical.
- `update` vs. `net_up` — both bare up-arrows today.
- `backlight` — series order currently inverted; fix direction.
- One consistent slash treatment across all "muted/off/disconnected" icons.
