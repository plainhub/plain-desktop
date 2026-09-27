//! UI helpers and a `bash -c` runner with a TTY spinner. Mirrors
//! `cmd/install/ui.go` and `internal/pkg/cmd/progress.go` in the Go side.
//!
//! Behaviour:
//!   * On a TTY: prints a single updating line, then a success/failure
//!     line with elapsed time.
//!   * On a non-TTY: prints start + end lines (no spinner).
//!   * Captures up to 64 KiB of stdout/stderr; on error, prints the tail.

use std::io::{IsTerminal, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_GREEN: &str = "\x1b[32m";
const COLOR_YELLOW: &str = "\x1b[33m";
const COLOR_RED: &str = "\x1b[31m";
const COLOR_CYAN: &str = "\x1b[36m";

pub fn print_section(title: &str) {
    println!("\n{}{}== {} =={}", COLOR_CYAN, "\n", title, COLOR_RESET);
}

pub fn print_ok(title: &str, detail: &str) {
    print_line(&format!("{COLOR_GREEN}OK{COLOR_RESET}"), title, detail);
}
pub fn print_note(title: &str, detail: &str) {
    print_line(&format!("{COLOR_YELLOW}NOTE{COLOR_RESET}"), title, detail);
}
pub fn print_fail(title: &str, detail: &str) {
    print_line(&format!("{COLOR_RED}FAIL{COLOR_RESET}"), title, detail);
}

fn print_line(status: &str, title: &str, detail: &str) {
    let detail = detail.trim();
    if detail.is_empty() {
        println!("[{status}] {title}");
    } else {
        println!("[{status}] {title}: {detail}");
    }
}

fn is_tty() -> bool {
    std::io::stdout().is_terminal()
}

/// Run a shell command via `bash -c`, streaming the spinner to stdout on
/// TTY. Returns `Ok(())` on success, `Err(err)` otherwise. The command
/// output is captured (up to 64 KiB) and printed on failure.
pub fn run_progress(label: &str, command: &str) -> Result<(), String> {
    let start = Instant::now();
    let output = Arc::new(Mutex::new(Vec::with_capacity(64 * 1024)));
    let writer = Arc::clone(&output);

    let mut child = Command::new("bash")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;

    // Tap stdout + stderr into the shared output buffer.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let w1 = Arc::clone(&writer);
    let t1 = thread::spawn(move || {
        if let Some(s) = stdout.as_mut() {
            let _ = pipe_into(s, &w1);
        }
    });
    let w2 = Arc::clone(&writer);
    let t2 = thread::spawn(move || {
        if let Some(s) = stderr.as_mut() {
            let _ = pipe_into(s, &w2);
        }
    });

    let tty = is_tty();
    let spinner_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let spinner_handle = if tty {
        let stop = Arc::clone(&spinner_stop);
        let label = label.to_string();
        Some(thread::spawn(move || {
            let frames = ['|', '/', '-', '\\'];
            let mut i = 0usize;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let elapsed = start.elapsed().as_secs();
                print!(
                    "\r\x1b[2K{}... {} {}s",
                    label,
                    frames[i % frames.len()],
                    elapsed
                );
                let _ = std::io::stdout().flush();
                i += 1;
                thread::sleep(Duration::from_millis(120));
            }
            print!("\r\x1b[2K");
            let _ = std::io::stdout().flush();
        }))
    } else {
        println!("- {label}...");
        None
    };

    let result = child.wait();
    let _ = t1.join();
    let _ = t2.join();
    spinner_stop.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Some(h) = spinner_handle {
        let _ = h.join();
    }

    let elapsed = start.elapsed();
    match result {
        Ok(status) if status.success() => {
            println!("\u{2713} {label} ({}ms)", elapsed.as_millis());
            Ok(())
        }
        Ok(status) => {
            println!(
                "\u{2717} {label} ({}ms, exit {:?})",
                elapsed.as_millis(),
                status.code()
            );
            let tail = {
                let g = output.lock().unwrap();
                String::from_utf8_lossy(&g).to_string()
            };
            let trimmed = tail.trim();
            if !trimmed.is_empty() {
                println!("---- command output (tail) ----");
                println!("{trimmed}");
                println!("------------------------------");
            }
            Err(format!("command exited with {status:?}"))
        }
        Err(e) => {
            println!("\u{2717} {label} ({}ms, spawn: {e})", elapsed.as_millis());
            Err(format!("spawn: {e}"))
        }
    }
}

fn pipe_into<R: Read>(src: &mut R, buf: &Arc<Mutex<Vec<u8>>>) -> std::io::Result<()> {
    let mut chunk = [0u8; 4096];
    loop {
        let n = src.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        let mut g = buf.lock().unwrap();
        let cur_len = g.len();
        if cur_len + n <= 64 * 1024 {
            g.extend_from_slice(&chunk[..n]);
        } else {
            // keep only the last 64 KiB.
            let keep: usize = 64 * 1024usize.saturating_sub(n);
            if cur_len > keep {
                let drop_n = cur_len - keep;
                g.drain(..drop_n);
            }
            g.extend_from_slice(&chunk[..n]);
        }
    }
    Ok(())
}

use std::io::Read;
