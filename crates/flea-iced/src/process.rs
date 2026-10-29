// One argv run off the UI thread under a deadline. A child still running at the deadline is killed
// and reported as TimedOut, which is what bounds every gio and lsblk call the rail makes: each one
// that went unbounded in QML froze a whole group for the life of the window (AGENTS.md "A
// single-flight guard needs a deadline").
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub enum Ran {
    // The exit code, None when a signal ended it, and what it wrote to stdout.
    Exited(Option<i32>, String),
    TimedOut,
    // It never started: a missing program is a refusal, not a hang.
    NotStarted,
}

// gio's own output is pinned to C so a decision never reads a translated sentence (issue #36).
pub const GIO_ENV: [(&str, &str); 1] = [("LC_ALL", "C")];

pub fn run_within(argv: &[String], env: &[(&str, &str)], deadline: Duration) -> Ran {
    run_with_input(argv, env, None, deadline)
}

// The same, with one line written to stdin and the pipe closed: how a password reaches
// tools/flea-gio-auth without ever being an argument another process could read.
pub fn run_with_input(argv: &[String], env: &[(&str, &str)], input: Option<String>, deadline: Duration) -> Ran {
    let Some((program, args)) = argv.split_first() else { return Ran::NotStarted };
    let mut cmd = Command::new(program);
    let stdin = if input.is_some() { Stdio::piped() } else { Stdio::null() };
    cmd.args(args).stdin(stdin).stdout(Stdio::piped()).stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let Ok(mut child) = cmd.spawn() else { return Ran::NotStarted };
    if let (Some(text), Some(mut pipe)) = (input, child.stdin.take()) {
        // A helper that exits before reading is its own answer, so a failed write is not an error.
        let _ = std::io::Write::write_all(&mut pipe, text.as_bytes());
    }
    let Some(mut stdout) = child.stdout.take() else { return Ran::NotStarted };
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout, &mut s);
        s
    });
    let end = Instant::now() + deadline;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ran::Exited(status.code(), reader.join().unwrap_or_default()),
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Ran::TimedOut;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_answers_a_hang_is_killed_and_a_missing_program_never_starts() {
        let sh = |s: &str| vec!["sh".to_string(), "-c".to_string(), s.to_string()];
        assert_eq!(run_within(&sh("echo $LC_ALL; exit 3"), &GIO_ENV, Duration::from_secs(5)), Ran::Exited(Some(3), "C\n".into()));
        let started = Instant::now();
        assert_eq!(run_within(&sh("sleep 5"), &[], Duration::from_millis(200)), Ran::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(run_within(&["/nonexistent/flea-test".to_string()], &[], Duration::from_secs(1)), Ran::NotStarted);
    }
}
