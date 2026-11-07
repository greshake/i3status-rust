//! Terminal images for doctor: lets a terminal that speaks the kitty graphics
//! protocol show glyphs as the bar draws them, not as the terminal's own
//! fonts would.
//!
//! Detection follows kitty's documentation: a graphics query, then a cell
//! size request, then a Primary Device Attributes (DA) request. Every
//! terminal answers DA, so DA is the sentinel and a terminal that ignores
//! the rest never makes us wait for the timeout.
//!
//! ```text
//!   us  ──► ESC _G i=31,a=q,… ESC \   ESC [ 16 t   ESC [ c
//!   tty ◄── ESC _G i=31;OK ESC \       (only with kitty graphics)
//!   tty ◄── ESC [ 6 ; 20 ; 10 t        (cell size, when supported)
//!   tty ◄── ESC [ ? 62 ; 22 c          (always)
//! ```

use std::fs::OpenOptions;
use std::io::{IsTerminal as _, Read as _, Write as _};
use std::os::fd::AsRawFd as _;
use std::time::{Duration, Instant};

use base64::Engine as _;

/// A 1×1 query-only image (`a=q`), the cell size request, and DA.
const QUERY: &[u8] = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[16t\x1b[c";
const KITTY_OK: &[u8] = b"\x1b_Gi=31;OK";
const DA_START: &[u8] = b"\x1b[?";
const CELL_SIZE_START: &[u8] = b"\x1b[6;";
const REPLY_TIMEOUT: Duration = Duration::from_millis(500);

/// The protocol caps each escape's payload at 4096 bytes of base64.
const CHUNK: usize = 4096;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// One terminal cell, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
    pub width: u32,
    pub height: u32,
}

/// The cell size of a terminal that can show kitty images on stdout, or
/// `None` when doctor has to stay text-only: stdout is not a terminal, the
/// terminal has no kitty graphics (alacritty, tmux without passthrough), or
/// it does not report its cell size.
pub fn kitty_cell_size() -> Option<CellSize> {
    if !std::io::stdout().is_terminal() {
        return None;
    }
    let reply = query_tty().ok()?;
    find(&reply, KITTY_OK)?;
    cell_size(&reply)
}

/// A PNG drawn one cell high where the cursor is, and how many cells wide it
/// comes out. The cursor does not move (`C=1`): the caller pads with spaces,
/// so table alignment never depends on the terminal moving it right.
pub fn png_image(png: &[u8], cell: CellSize) -> Option<(String, usize)> {
    let (width, height) = png_size(png)?;
    let cells = cells_wide(width, height, cell);
    Some((escape(png), cells))
}

/// The escapes that place `png`: one row high, no cursor movement, no
/// replies (`q=2`, which would otherwise arrive as input). Payloads longer
/// than [`CHUNK`] continue in `m=1` escapes.
fn escape(png: &[u8]) -> String {
    let payload = base64::engine::general_purpose::STANDARD.encode(png);
    let chunks: Vec<&[u8]> = payload.as_bytes().chunks(CHUNK).collect();

    let mut out = String::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        let keys = if i == 0 {
            format!("a=T,f=100,r=1,C=1,q=2,m={more}")
        } else {
            format!("m={more}")
        };
        // base64 is ASCII.
        let chunk = std::str::from_utf8(chunk).unwrap_or_default();
        out.push_str(&format!("\x1b_G{keys};{chunk}\x1b\\"));
    }
    out
}

/// Width and height from the PNG header (IHDR follows the signature).
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if !png.starts_with(PNG_SIGNATURE) {
        return None;
    }
    let dimension = |at: usize| Some(u32::from_be_bytes(png.get(at..at + 4)?.try_into().ok()?));
    Some((dimension(16)?, dimension(20)?))
}

/// Cells an image covers once scaled to one cell high, rounded up.
fn cells_wide(width: u32, height: u32, cell: CellSize) -> usize {
    if height == 0 || cell.width == 0 {
        return 1;
    }
    let scaled = u64::from(width) * u64::from(cell.height) / u64::from(height);
    scaled.div_ceil(u64::from(cell.width)).max(1) as usize
}

/// Raw mode for the duration of the probe: no line buffering, no echo of the
/// terminal's reply. Restores the previous mode when dropped.
struct RawMode {
    fd: i32,
    saved: libc::termios,
}

impl RawMode {
    fn enter(fd: i32) -> std::io::Result<Self> {
        // SAFETY: tcgetattr only writes into the termios we pass it.
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
            return Err(std::io::Error::last_os_error());
        }

        let mut raw = saved;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        // SAFETY: fd is an open terminal and raw is a valid termios.
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { fd, saved })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        // SAFETY: restoring the termios read from this same fd.
        unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, &self.saved) };
    }
}

/// Send [`QUERY`] to the controlling terminal and collect the reply up to
/// the DA answer. Fails when there is no terminal (piped, cron, CI).
fn query_tty() -> std::io::Result<Vec<u8>> {
    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    let fd = tty.as_raw_fd();
    let _raw = RawMode::enter(fd)?;

    tty.write_all(QUERY)?;
    tty.flush()?;

    // Read until the DA reply is complete, or give up at the deadline.
    let deadline = Instant::now() + REPLY_TIMEOUT;
    let mut reply = Vec::new();
    while !da_complete(&reply) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || !readable(fd, left)? {
            break;
        }

        let mut buf = [0u8; 256];
        let n = tty.read(&mut buf)?;
        if n == 0 {
            break;
        }
        reply.extend_from_slice(&buf[..n]);
    }
    Ok(reply)
}

/// Whether `fd` has input within `timeout`.
fn readable(fd: i32, timeout: Duration) -> std::io::Result<bool> {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
    // SAFETY: one valid pollfd.
    match unsafe { libc::poll(&mut pfd, 1, ms) } {
        -1 => Err(std::io::Error::last_os_error()),
        0 => Ok(false),
        _ => Ok(true),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Whether the reply holds a full DA response: `ESC [ ? … c`.
fn da_complete(reply: &[u8]) -> bool {
    find(reply, DA_START).is_some_and(|start| reply[start..].contains(&b'c'))
}

/// The cell size reply: `ESC [ 6 ; height ; width t`.
fn cell_size(reply: &[u8]) -> Option<CellSize> {
    let start = find(reply, CELL_SIZE_START)? + CELL_SIZE_START.len();
    let body = &reply[start..];
    let end = body.iter().position(|&b| b == b't')?;

    let text = std::str::from_utf8(&body[..end]).ok()?;
    let (height, width) = text.split_once(';')?;
    let cell = CellSize {
        width: width.parse().ok()?,
        height: height.parse().ok()?,
    };
    (cell.width > 0 && cell.height > 0).then_some(cell)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: CellSize = CellSize {
        width: 10,
        height: 20,
    };

    /// A PNG header for a `width`×`height` image; only IHDR is read.
    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend([0, 0, 0, 13]);
        png.extend(b"IHDR");
        png.extend(width.to_be_bytes());
        png.extend(height.to_be_bytes());
        png
    }

    #[test]
    fn cell_size_is_read_from_the_reply() {
        let reply = b"\x1b_Gi=31;OK\x1b\\\x1b[6;20;10t\x1b[?62;22c";
        assert_eq!(cell_size(reply), Some(CELL));
    }

    #[test]
    fn no_cell_size_without_the_reply() {
        // e.g. a terminal that answers DA only
        assert_eq!(cell_size(b"\x1b[?6c"), None);
        assert_eq!(cell_size(b"\x1b[6;0;10t"), None);
    }

    #[test]
    fn da_is_complete_only_once_terminated() {
        assert!(!da_complete(b"\x1b[?62;4"));
        assert!(da_complete(b"\x1b[?62;4c"));
    }

    #[test]
    fn png_size_is_read_from_the_header() {
        assert_eq!(png_size(&png_header(15, 17)), Some((15, 17)));
        assert_eq!(png_size(b"GIF89a"), None);
        assert_eq!(png_size(PNG_SIGNATURE), None);
    }

    #[test]
    fn width_in_cells_follows_the_one_row_scale() {
        // 15×15 at a 20px row is 20px wide: two 10px cells.
        assert_eq!(cells_wide(15, 15, CELL), 2);
        // 30×15 doubles: 40px, four cells.
        assert_eq!(cells_wide(30, 15, CELL), 4);
        // A sliver still takes a cell.
        assert_eq!(cells_wide(1, 15, CELL), 1);
    }

    #[test]
    fn image_is_placed_without_moving_the_cursor_or_replying() {
        let (escape, cells) = png_image(&png_header(15, 15), CELL).unwrap();
        assert_eq!(cells, 2);
        assert!(escape.starts_with("\x1b_Ga=T,f=100,r=1,C=1,q=2,m=0;"));
        assert!(escape.ends_with("\x1b\\"));
    }

    #[test]
    fn long_payloads_continue_in_chunks() {
        let png = vec![0u8; CHUNK]; // base64 grows it past one chunk
        let escape = escape(&png);
        let parts: Vec<&str> = escape.split("\x1b\\").filter(|p| !p.is_empty()).collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].starts_with("\x1b_Ga=T,f=100,r=1,C=1,q=2,m=1;"));
        assert!(parts[1].starts_with("\x1b_Gm=0;"));
    }
}
