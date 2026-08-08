# i3status Icons

The icon font behind the `i3status-icons` icon set. Drawn for this project, so
it covers every icon name i3status-rs uses, progressions included.

| | |
|---|---|
| Family name | `i3status Icons` (exactly — this is what `fc-match` and the bar's `font` directive need) |
| Glyphs | 99 |
| Codepoints | U+E900–U+E962, Private Use Area |
| Units per em | 1000, ascent 850, descent −150 |
| Sources | `svg/`, 99 files, 24×24 viewBox, 2px stroke, `currentColor` |

Using it is documented in [`doc/themes.md`](../../doc/themes.md#available-icon-sets).
This file is for changing it.

## Rebuilding

```shell
$ cargo xtask build-font
```

That reads `svg/*.svg` and rewrites two files:

- `i3status-icons.ttf` — the font
- `../../files/icons/i3status-icons.toml` — the icon set that maps icon names to codepoints

Both are checked in, so neither users nor packagers ever need to run this. Only
run it when you change the SVGs.

The build is pure Rust and needs no tools beyond cargo. It parses each SVG with
`usvg`, converts strokes to filled outlines with the `tiny-skia` stroker,
converts cubics to the quadratics the `glyf` table requires with `kurbo`, and
assembles the tables with `write-fonts`. The canonical icon names come from
`Icons::default()` in `src/icons.rs`, so the icon set cannot drift from the code
it has to match — a name with no SVG, or an SVG with no name, fails the build.

## Codepoints are a compatibility contract

Codepoints are assigned from U+E900 **in sorted filename order**. They are baked
into the generated icon set, and users can reference them directly in
`icons_overrides` (see [`doc/themes.md`](../../doc/themes.md#available-icon-overrides)).

So: **adding an SVG whose name does not sort last shifts every codepoint after
it**, silently changing what existing configurations render. `airplane.svg`
would renumber almost the whole font.

Until this is replaced by a checked-in codepoint map, treat it as a rule: when
adding a glyph, make sure its filename sorts after every existing one, or accept
that you are making a breaking change and say so in the pull request.

## Naming and progressions

One SVG per icon state. A file named `<name>_<digits>` is one step of a
progression; the steps are collected into a TOML array ordered by the number:

```
bat_0.svg  bat_1.svg  bat_2.svg  bat_3.svg  bat_4.svg
    ->  bat = ["\ue905", "\ue906", "\ue907", "\ue908", "\ue909"]
```

Low index means less ink means less of the thing — an empty battery, a weak
signal, a quiet speaker. Any single step has to be readable on its own, because
the user only ever sees one at a time.

The digits matter: `bat_charging.svg` is **not** a step of `bat`, because
`charging` is not a number. That is the only thing separating the two cases, so
a future icon name ending in `_<digit>` would be grouped by mistake.

Everything else is a single glyph named exactly after the icon.

## Adding or changing an icon

1. Draw it as a 24×24 SVG with a 2px stroke, using `currentColor` so the bar's
   own colours apply. Match the existing weight and corner treatment — a dozen
   of these sit side by side all day.
2. Save it in `svg/`. For a new icon, see the codepoint warning above.
3. For a new icon, add the name to `Icons::default()` in `src/icons.rs`.
4. `cargo xtask build-font`
5. `./verify_icon_files.sh` and `cargo test` — both check the icon set against
   the canonical names.
6. Install it and look at it: `install.sh` copies the font to
   `$XDG_DATA_HOME/fonts`. Run `fc-cache -f` and check `fc-match "i3status Icons"`
   before blaming the glyph — a stale font cache looks exactly like a bad build.

Strokes are not outlined in the sources — the build converts them. Design at the
size these are used, roughly the height of a line of text: interior detail that
does not survive being shrunk to an x-height should go.

[`design-spec.md`](design-spec.md) records what each icon is meant to depict and
why, including the pairs that intentionally share a silhouette. Read it before
redrawing anything.

## Provenance

The SVG sources were drawn for i3status-rust and contributed under the GNU
General Public License version 3 only (GPL-3.0-only), the same licence as the
rest of the project. See [`LICENSE`](../../LICENSE).
