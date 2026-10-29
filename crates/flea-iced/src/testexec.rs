// A stub a test has just written can be refused with ETXTBSY: a sibling test's fork that happened
// while the stub's write descriptor was open still holds it until that child execs, close-on-exec
// notwithstanding. A fork after the write closed cannot hold it, so once one spawn of the stub
// succeeds no later one can be refused. Measured: one red in a full parallel run, none alone.
use std::process::{Command, Stdio};
use std::time::Duration;

const ETXTBSY: i32 = 26;

pub fn wait_executable(path: &std::path::Path) {
    for _ in 0..200 {
        match Command::new(path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status() {
            Err(e) if e.raw_os_error() == Some(ETXTBSY) => std::thread::sleep(Duration::from_millis(10)),
            _ => return,
        }
    }
    panic!("{} stayed busy for two seconds", path.display());
}
