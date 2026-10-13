use std::io::{self, BufRead, Read, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;

pub fn prompt_password(prompt: &str) -> io::Result<String> {
    let _ = write!(io::stderr(), "{prompt}");
    let _ = io::stderr().flush();
    let mut line = read_password_line()?;
    while matches!(line.last(), Some(b'\n') | Some(b'\r')) {
        line.pop();
    }
    String::from_utf8(line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(unix)]
fn read_password_line() -> io::Result<Vec<u8>> {
    use std::fs::File;
    use std::io::BufReader;
    use std::os::fd::{FromRawFd, IntoRawFd};

    let source = match File::open("/dev/tty") {
        Ok(f) => Source::Tty(f.into()),
        Err(_) => Source::Stdin(io::stdin()),
    };
    let raw = source.as_raw_fd();
    let owns_fd = source.owns_fd();

    if owns_fd {
        set_echo(raw, false)?;
    }

    let mut file = unsafe { File::from_raw_fd(raw) };
    let mut reader = BufReader::new(LeakFd::new(&mut file));
    let mut buf = Vec::with_capacity(64);
    let read_result = reader.read_until(b'\n', &mut buf);

    if owns_fd {
        let _ = set_echo(raw, true);
        let _ = write!(io::stderr(), "\n");
        let _ = io::stderr().flush();
    }

    let raw2 = file.into_raw_fd();
    if owns_fd {
        let close_result = unsafe { libc::close(raw2) };
        if read_result.is_ok() && close_result != 0 {
            return Err(io::Error::last_os_error());
        }
    }

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

#[cfg(not(unix))]
fn read_password_line() -> io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(64);
    io::stdin().lock().read_until(b'\n', &mut buf)?;
    Ok(buf)
}
