//! Ratatui adapter that keeps cursor queries off the command's stdout.

use std::io::{self, Stderr};

use ratatui::{
    backend::{Backend, ClearType, CrosstermBackend, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};

pub struct TerminalBackend(CrosstermBackend<Stderr>);

impl TerminalBackend {
    pub fn new() -> Self {
        Self(CrosstermBackend::new(io::stderr()))
    }
}

impl Backend for TerminalBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.0.draw(content)
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        self.0.hide_cursor()
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        self.0.show_cursor()
    }
    fn get_cursor_position(&mut self) -> io::Result<Position> {
        #[cfg(unix)]
        return cursor_position();
        #[cfg(not(unix))]
        self.0.get_cursor_position()
    }
    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.0.set_cursor_position(position)
    }
    fn clear(&mut self) -> io::Result<()> {
        self.0.clear()
    }
    fn clear_region(&mut self, kind: ClearType) -> io::Result<()> {
        self.0.clear_region(kind)
    }
    fn append_lines(&mut self, count: u16) -> io::Result<()> {
        self.0.append_lines(count)
    }
    fn size(&self) -> io::Result<Size> {
        self.0.size()
    }
    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.0.window_size()
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(unix)]
fn cursor_position() -> io::Result<Position> {
    use std::{
        fs::OpenOptions,
        io::{Read, Write},
        time::{Duration, Instant},
    };

    // A separate descriptor avoids altering flags on stdin or stderr. A bounded
    // wait restores the terminal promptly if it does not answer the query.
    let mut tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
    tty.write_all(b"\x1b[6n")?;
    let flags = rustix::fs::fcntl_getfl(&tty)?;
    rustix::fs::fcntl_setfl(&tty, flags | rustix::fs::OFlags::NONBLOCK)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut response = Vec::with_capacity(24);
    let mut byte = [0];
    while Instant::now() < deadline {
        match tty.read(&mut byte) {
            Ok(1) => {
                if byte[0] == 27 || response.len() >= 24 {
                    response.clear();
                }
                response.push(byte[0]);
                if byte[0] == b'R' {
                    if let Some(position) = parse_position(&response) {
                        return Ok(position);
                    }
                    response.clear();
                }
            }
            Ok(_) => std::thread::sleep(Duration::from_millis(5)),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "terminal did not report its cursor position",
    ))
}

#[cfg(unix)]
fn parse_position(response: &[u8]) -> Option<Position> {
    let response = std::str::from_utf8(response).ok()?;
    let (row, column) = response
        .strip_prefix("\x1b[")?
        .strip_suffix('R')?
        .split_once(';')?;
    Some(Position {
        x: column.parse::<u16>().ok()?.checked_sub(1)?,
        y: row.parse::<u16>().ok()?.checked_sub(1)?,
    })
}
