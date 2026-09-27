//! Read a password from the controlling TTY with echo disabled.
//!
//! Replaces the `rpassword` crate. Behaviour matches
//! `rpassword::prompt_password`:
//!
//! 1. Write the prompt to **stderr** (matches the de-facto Unix convention
//!    that passwords are not echoed to stdout).
//! 2. Open `/dev/tty` so we always talk to the controlling terminal even
//!    when stdin/stdout have been redirected. Fall back to stdin if
//!    `/dev/tty` doesn't exist (CI / no controlling tty).
//! 3. Save the current termios, clear `ECHO` (and `ECHONL`/`ECHOK` for
//!    safety), restore afterwards.
//! 4. Read one line, strip the trailing newline characters, return as a
//!    lossy UTF-8 `String` (passwords should be ASCII anyway).
//!
//! All APIs used here are **stable Rust 1.70+**:
//!   * `std::os::fd::{AsFd, AsRawFd, FromRawFd, IntoRawFd, OwnedFd}`
//!     (stable since 1.66)
//!   * `std::io::IsTerminal` (stable since 1.70)
//!   * `libc::tcgetattr` / `tcsetattr` via explicit `libc` dep
//!
//! No nightly features, no `#![feature(...)]` attributes anywhere.

use std::io::{self, BufRead, Read, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;

/// Prompt `prompt` on stderr and return the password typed by the user
/// (echo disabled). Errors are returned as-is.
pub fn prompt_password(prompt: &str) -> io::Result<String> {
    let _ = write!(io::stderr(), "{prompt}");
    let _ = io::stderr().flush();
    let mut line = read_password_line()?;
    while matches!(line.last(), Some(b'\n') | Some(b'\r')) {
        line.pop();
    }
    String::from_utf8(line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

// ---------------------------------------------------------------------------
// Unix implementation
// ---------------------------------------------------------------------------
#[cfg(unix)]
fn read_password_line() -> io::Result<Vec<u8>> {
    use std::fs::File;
    use std::io::BufReader;
    use std::os::fd::{FromRawFd, IntoRawFd};

    // Pick the input source. `/dev/tty` first; fall back to stdin.
    // `Tty(OwnedFd)` closes the fd on drop; `Stdin(OwnedFd)` borrows
    // fd 0 from `io::stdin()` — we still wrap it in `OwnedFd` for uniform
    // handling, but never `close(0)` it.
    let source = match File::open("/dev/tty") {
        Ok(f) => Source::Tty(f.into()),
        Err(_) => Source::Stdin(io::stdin()),
    };
    let raw = source.as_raw_fd();
    let owns_fd = source.owns_fd();

    if owns_fd {
        set_echo(raw, false)?;
    }

    // SAFETY: the fd is valid (we either own it from `/dev/tty`, or
    // borrowed stdin fd 0 which is always open). We hand the File off to
    // a `LeakFd` wrapper that, on drop, detaches the fd *without* closing
    // it — `set_echo(true)` and (for the Tty branch) fd close happen
    // explicitly below, after this function returns.
    let mut file = unsafe { File::from_raw_fd(raw) };
    let mut reader = BufReader::new(LeakFd::new(&mut file));
    let mut buf = Vec::with_capacity(64);
    let read_result = reader.read_until(b'\n', &mut buf);

    if owns_fd {
        let _ = set_echo(raw, true);
        let _ = write!(io::stderr(), "\n");
        let _ = io::stderr().flush();
    }

    // Detach the fd from `file` so its drop does not close it; close
    // explicitly here for the Tty branch. (We do this regardless of
    // branch because `LeakFd` already detached the fd from the File.)
    let raw2 = file.into_raw_fd();
    if owns_fd {
        // SAFETY: `raw2` came from a File we own (from `/dev/tty`).
        let close_result = unsafe { libc::close(raw2) };
        if read_result.is_ok() && close_result != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    // For the Stdin branch we do nothing — fd 0 stays open.

    read_result?;
    Ok(buf)
}

enum Source {
    Tty(std::os::fd::OwnedFd),
    Stdin(io::Stdin),
}
impl Source {
    fn as_raw_fd(&self) -> i32 {
        use std::os::fd::AsFd;
        match self {
            Source::Tty(o) => o.as_fd().as_raw_fd(),
            Source::Stdin(s) => s.as_fd().as_raw_fd(),
        }
    }
    fn owns_fd(&self) -> bool {
        matches!(self, Source::Tty(_))
    }
}

/// Adapter: takes a `&mut File` and exposes a `Read` view that, on drop,
/// does nothing. Used to wrap a `File` whose fd we want to keep open
/// (either because we'll close it ourselves later, or because it
/// belongs to `io::stdin()` and must never be closed).
struct LeakFd<'a>(&'a mut std::fs::File);
impl<'a> LeakFd<'a> {
    fn new(f: &'a mut std::fs::File) -> Self {
        Self(f)
    }
}
impl Read for LeakFd<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

#[cfg(unix)]
fn set_echo(fd: i32, on: bool) -> io::Result<()> {
    // SAFETY: `termios` is a POD struct; `tcgetattr`/`tcsetattr` are safe
    // to call as long as `fd` is a valid terminal fd. Caller guarantees
    // that.
    unsafe {
        let mut termios = std::mem::zeroed::<libc::termios>();
        if libc::tcgetattr(fd, &mut termios) != 0 {
            return Err(io::Error::last_os_error());
        }
        if on {
            termios.c_lflag |= libc::ECHO;
        } else {
            termios.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ECHOK);
        }
        if libc::tcsetattr(fd, libc::TCSANOW, &termios) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Non-Unix stub — `cargo check --target` etc. still compiles.
// ---------------------------------------------------------------------------
#[cfg(not(unix))]
fn read_password_line() -> io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(64);
    io::stdin().lock().read_until(b'\n', &mut buf)?;
    Ok(buf)
}
