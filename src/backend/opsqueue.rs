// A transfer asked for while another operation holds the slot waits here rather than being refused,
// and starts the moment the slot frees. The card still shows one transfer at a time, because the
// slot is still one; what changed is that a second copy is the operator's next intent, not an error.
// Only transfers queue: trash, duplicate and redo keep their refusal, and so does a shelf drop,
// whose token is spent the moment it is redeemed.
use crate::backend::menu_actions::Selected;
use crate::backend::opsdispatch::Ops;
use crate::backend::opsreq::{run_transfer_checked, transferdone_line, transferstarted_line};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;

// Everything a transfer needs to start later, resolved when it was asked for: the listing a rows form
// named, and the menu selection it came from, may both be gone by the time its turn comes.
pub(crate) struct Waiting {
    pub id: usize,
    pub moving: bool,
    pub paths: Vec<String>,
    pub dest: PathBuf,
    pub selection: Option<Vec<Selected>>,
    pub destination: Option<Selected>,
}

// ahead counts the running operation too, so the first transfer to wait answers 1.
pub fn transferqueued_line(id: usize, n: usize, moving: bool, ahead: usize) -> String {
    format!(r#"{{"t":"transferqueued","id":{},"n":{},"moving":{},"ahead":{}}}"#, id, n, moving, ahead)
}

// Starts now when the slot is free, waits otherwise; the id is the same either way, so the client
// follows one number from transferqueued through transferstarted to transferdone.
pub(crate) fn submit(out: &mut impl Write, ops: &mut Ops, waiting: Waiting) {
    if ops.live.running().is_none() {
        return launch(out, ops, waiting);
    }
    let ahead = ops.queue.len() + 1;
    writeln!(out, "{}", transferqueued_line(waiting.id, waiting.paths.len(), waiting.moving, ahead)).ok();
    out.flush().ok();
    ops.queue.push_back(waiting);
}

// Called after every terminal message, which is the only moment the slot can free.
pub(crate) fn pump(out: &mut impl Write, ops: &mut Ops) {
    if ops.live.running().is_some() {
        return;
    }
    if let Some(next) = ops.queue.pop_front() {
        launch(out, ops, next);
    }
}

// A waiting transfer has touched nothing, so cancelling it is taking it out of line; it still answers
// its own transferdone, because that is the one line a client already ends every transfer on.
pub(crate) fn cancel_waiting(out: &mut impl Write, ops: &mut Ops, id: usize) -> bool {
    let Some(at) = ops.queue.iter().position(|waiting| waiting.id == id) else { return false };
    let gone = ops.queue.remove(at).map(|waiting| waiting.paths.len()).unwrap_or(0);
    writeln!(out, "{}", transferdone_line(id, 0, 0, gone, true, &[])).ok();
    out.flush().ok();
    true
}

fn launch(out: &mut impl Write, ops: &mut Ops, waiting: Waiting) {
    let Waiting { id, moving, paths, dest, selection, destination } = waiting;
    ops.transfer_retry = (0, Vec::new());
    let cancel = Arc::new(AtomicBool::new(false));
    ops.live.claim(id, &cancel);
    writeln!(out, "{}", transferstarted_line(id, paths.len(), moving)).ok();
    out.flush().ok();
    let tx = ops.tx.clone();
    thread::spawn(move || run_transfer_checked(id, moving, paths, dest, cancel, tx, selection, destination));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::opsdispatch::{cancel_transfer, report_op, start_transfer};
    use crate::backend::opsreq::OpMsg;
    use crate::backend::testdir::TestDir;
    use crate::backend::undo::Entry;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    fn text(buf: &[u8]) -> String {
        String::from_utf8_lossy(buf).to_string()
    }

    fn done(id: usize) -> OpMsg {
        OpMsg::TransferDone { id, ok: 1, failed: 0, skipped: 0, cancelled: false,
                              entry: Entry { op: "copy".into(), steps: Vec::new() }, retry: Vec::new() }
    }

    #[test]
    fn a_transfer_asked_for_while_one_runs_waits_and_starts_when_the_slot_frees() {
        let d = TestDir::new("queuewaits");
        let (tx, rx) = channel();
        let mut o = Ops::new(tx);
        let (running, _) = o.claim_transfer();
        let source = d.file("a.txt", "a");
        let dest = d.dir("out");
        let mut buf = Vec::new();
        start_transfer(&mut buf, &mut o, "copy", vec![source.to_string_lossy().into()], &dest.to_string_lossy());
        let queued = running + 1;
        assert_eq!(text(&buf).trim(), transferqueued_line(queued, 1, false, 1));
        assert!(!dest.join("a.txt").exists(), "a waiting transfer touches nothing");
        assert_eq!(o.live.running(), Some(running), "and does not take the slot from the one running");
        buf.clear();
        report_op(&mut buf, &mut o, done(running));
        let lines = text(&buf);
        let finished = lines.find(r#""t":"transferdone""#).expect("the running one ends first");
        let started = lines.find(&transferstarted_line(queued, 1, false)).expect("then the waiting one starts");
        assert!(finished < started, "{}", lines);
        assert_eq!(o.live.running(), Some(queued));
        let mut ended = false;
        while let Ok(message) = rx.recv_timeout(Duration::from_secs(5)) {
            ended = matches!(message, OpMsg::TransferDone { id, ok: 1, .. } if id == queued);
            if ended { break; }
        }
        assert!(ended && dest.join("a.txt").exists(), "the queued copy ran once its turn came");
    }

    #[test]
    fn transfers_wait_in_the_order_they_were_asked_for() {
        let d = TestDir::new("queueorder");
        let mut o = Ops::new(channel().0);
        let (running, _) = o.claim_transfer();
        let dest = d.dir("out").to_string_lossy().to_string();
        let mut buf = Vec::new();
        start_transfer(&mut buf, &mut o, "copy", vec![d.file("a", "a").to_string_lossy().into()], &dest);
        start_transfer(&mut buf, &mut o, "move", vec![d.file("b", "b").to_string_lossy().into()], &dest);
        assert!(text(&buf).contains(&transferqueued_line(running + 2, 1, true, 2)), "{}", text(&buf));
        let order: Vec<usize> = o.queue.iter().map(|waiting| waiting.id).collect();
        assert_eq!(order, vec![running + 1, running + 2]);
    }

    #[test]
    fn cancelling_a_waiting_transfer_takes_it_out_of_line_and_answers_its_done() {
        let d = TestDir::new("queuecancel");
        let mut o = Ops::new(channel().0);
        let (running, flag) = o.claim_transfer();
        let dest = d.dir("out");
        let mut buf = Vec::new();
        start_transfer(&mut buf, &mut o, "copy", vec![d.file("a", "a").to_string_lossy().into()], &dest.to_string_lossy());
        buf.clear();
        cancel_transfer(&mut buf, &mut o, running + 1);
        assert_eq!(text(&buf).trim(), transferdone_line(running + 1, 0, 0, 1, true, &[]));
        assert!(o.queue.is_empty());
        assert!(!flag.load(std::sync::atomic::Ordering::Relaxed), "the running one is not the one cancelled");
        buf.clear();
        report_op(&mut buf, &mut o, done(running));
        assert!(!text(&buf).contains("transferstarted"), "nothing is left to start");
        assert!(o.live.running().is_none());
        assert!(!dest.join("a").exists());
    }

    #[test]
    fn a_transfer_with_no_usable_destination_is_refused_rather_than_queued() {
        let mut o = Ops::new(channel().0);
        o.claim_transfer();
        let mut buf = Vec::new();
        start_transfer(&mut buf, &mut o, "copy", vec!["/etc/hostname".into()], "relative/dest");
        assert!(text(&buf).contains(r#""t":"error""#), "{}", text(&buf));
        assert!(o.queue.is_empty());
    }
}
