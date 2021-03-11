# Formatting

## Syntax

The syntax for placeholders is

```
{<name>[:[0]<min width>][^<max width>][;<min prefix>][*<unit>][#<bar max value>]}
```

### `<name>`

This is just a name of a placeholder. Each block that uses formatting will list them under "Available Format Keys" section of their config.

### `[0]<min width>`

Sets the minimum width of the content (in characters). If starts with a zero, `0` symbol will be used to pad the content. A space is used otherwise. Floats and Integers are shifted to the right, while Strings are to the left. Defaults to `0` for Strings, `2` for Integers and `3` for Floats.

#### Examples (□ is used instead of spaces)

`"{var:3}"`

The value of `var` | Output
-------------------|--------
`"abc"`            | `"abc"`
`"abcde"`          | `"abcde"`
`"ab"`             | `"ab□"`
`1`                | `"□□1"`
`1234`             | `"1234"`
`1.0`              | `"1.0"`
`12.0`             | `"□12"`
`123.0`            | `"123"`
`1234.0`           | `"1234"`

### `<max width>`

Sets the maximum width of the content (in characters). Applicable only for Strings. 

#### Examples

`"{var^3}"`

The value of `var` | Output
-------------------|--------
`"abc"`            | `"abc"`
`"abcde"`          | `"abc"`
`"ab"`             | `"ab"`

### `<min prefix>`

Float values are formatted following [engineering notation](https://en.wikipedia.org/wiki/Engineering_notation). This option sets the minimal SI prefix to use. The default value is `1` (no prefix) for bytes/bits and `n` (for nano) for everything else. Possible values are `n`, `u`, `m`, `1`, `K`, `M`, `G` and `T`.

#### Examples

`"{var:3;n}"`

The value of `var` | Output
-------------------|--------
`0.0001`           | "100u"
`0.001`            | "1.0m"
`0.01`             | " 10m"
`0.1`              | "100m"
`1.0`              | "1.0"
`12.0`             | " 12"
`123.0`            | "123"
`1234.0`           | "1.23K"

`"{var:3;1}"`

The value of `var` | Output
-------------------|--------
`0.0001`           | "0.0"
`0.001`            | "0.0"
`0.01`             | "0.0"
`0.1`              | "0.1"
`1.0`              | "1.0"
`12.0`             | " 12"
`123.0`            | "123"
`1234.0`           | "1.23K"

### `<unit>`

Some placeholders have a "unit". For example, `net` block displays speed in `B/s`. This option gives abitity to convert one units into another. Ignored for strings.

#### Example

`"{speed_down*b/s}"` - show the download speed in bits per second.

### `<bar max value>`

Every numeric placeholder (Integers and Floats) can be drawn as a bar. This option sets the value to be considered "100%". If this option is set, every other option will be ignored, except for `min width`, which will set the length of a bar.

#### Example

```toml
[[block]]
block = "sound"
format = "{volume:5#110} {volume:03}"
```

Here, `{volume:5#110}` means "draw a bar, 5 character long, with 100% being 110.

Output: https://imgur.com/a/CCNw04e
