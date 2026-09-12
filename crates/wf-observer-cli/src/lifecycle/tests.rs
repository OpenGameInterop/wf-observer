use std::{cell::RefCell, collections::BTreeSet, rc::Rc, sync::mpsc};

use anyhow::Context as _;
use tokio::time::Instant;

use crate::runtime::{Activity, HostStatus, RecordedProcess, TargetInfo};

use super::{
    backend::Backend,
    worker::{DISCOVERY_INTERVAL, Lifecycle, RETRY_INTERVAL},
};

struct FakeAttachment {
    process: RecordedProcess,
    dropped: Rc<RefCell<Vec<RecordedProcess>>>,
}

impl Drop for FakeAttachment {
    fn drop(&mut self) {
        self.dropped.borrow_mut().push(self.process);
    }
}

struct FakeBackend {
    candidates: Vec<RecordedProcess>,
    discovery_fails: bool,
    attachment_failures: BTreeSet<RecordedProcess>,
    live: BTreeSet<RecordedProcess>,
    discoveries: usize,
    attempts: Vec<RecordedProcess>,
    dropped: Rc<RefCell<Vec<RecordedProcess>>>,
    during_attach: Option<Box<dyn FnOnce()>>,
}

impl FakeBackend {
    fn new(candidates: Vec<RecordedProcess>) -> Self {
        Self {
            live: candidates.iter().copied().collect(),
            candidates,
            discovery_fails: false,
            attachment_failures: BTreeSet::new(),
            discoveries: 0,
            attempts: Vec::new(),
            dropped: Rc::default(),
            during_attach: None,
        }
    }
}

impl Backend for FakeBackend {
    type Candidate = RecordedProcess;
    type Attachment = FakeAttachment;

    fn discover(&mut self) -> anyhow::Result<Vec<RecordedProcess>> {
        self.discoveries += 1;
        anyhow::ensure!(!self.discovery_fails, "discovery failed");
        Ok(self.candidates.clone())
    }

    fn target(candidate: &RecordedProcess) -> TargetInfo {
        TargetInfo {
            process: *candidate,
            executable: format!("fixture-{}", candidate.pid),
            provider_id: format!("fixture-provider-{}", candidate.pid),
            game_id: format!("fixture-game-{}", candidate.pid),
        }
    }

    fn attach(&mut self, target: &RecordedProcess) -> anyhow::Result<FakeAttachment> {
        self.attempts.push(*target);
        if let Some(during_attach) = self.during_attach.take() {
            during_attach();
        }
        anyhow::ensure!(
            !self.attachment_failures.contains(target),
            "attachment failed"
        );
        Ok(FakeAttachment {
            process: *target,
            dropped: self.dropped.clone(),
        })
    }

    fn is_current(&mut self, attachment: &FakeAttachment) -> bool {
        self.live.contains(&attachment.process)
    }
}

const fn process(pid: u32, start_marker: u64) -> RecordedProcess {
    RecordedProcess { pid, start_marker }
}

fn step(
    lifecycle: &mut Lifecycle<FakeBackend>,
    stop: &mpsc::Receiver<()>,
    now: Instant,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        lifecycle.step(&mut |_| Ok(()), stop, &|| now)?.is_some(),
        "service stopped unexpectedly"
    );
    Ok(())
}

fn activity(
    lifecycle: &Lifecycle<FakeBackend>,
    process: RecordedProcess,
) -> anyhow::Result<Activity> {
    lifecycle
        .targets
        .get(&process)
        .map(|target| target.status.activity.clone())
        .context("target missing")
}

#[test]
fn attaches_all_processes_once_and_discovers_new_arrivals() -> anyhow::Result<()> {
    let (_stop_tx, stop) = mpsc::channel();
    let now = Instant::now();
    let first = process(10, 1);
    let second = process(20, 1);
    let third = process(30, 1);
    let mut lifecycle = Lifecycle::new(FakeBackend::new(vec![]));
    step(&mut lifecycle, &stop, now)?;
    assert_eq!(lifecycle.status(), HostStatus::default());

    lifecycle.backend.candidates = vec![first, second, first];
    lifecycle.backend.live.extend([first, second]);
    step(&mut lifecycle, &stop, now + DISCOVERY_INTERVAL)?;
    let initial = lifecycle.status();
    assert_eq!(initial.targets.len(), 2);
    assert!(
        initial
            .targets
            .iter()
            .all(|target| matches!(target.activity, Activity::Observing { .. }))
    );

    step(&mut lifecycle, &stop, now + DISCOVERY_INTERVAL * 2)?;
    assert_eq!(lifecycle.status(), initial);
    assert_eq!(lifecycle.backend.attempts, [first, second]);

    lifecycle.backend.candidates.push(third);
    lifecycle.backend.live.insert(third);
    step(&mut lifecycle, &stop, now + DISCOVERY_INTERVAL * 3)?;
    assert_eq!(lifecycle.backend.attempts, [first, second, third]);
    assert_eq!(lifecycle.backend.discoveries, 4);
    assert_eq!(lifecycle.status().targets[..2], initial.targets);
    Ok(())
}

#[test]
fn exit_and_pid_reuse_only_replace_the_affected_session() -> anyhow::Result<()> {
    let (_stop_tx, stop) = mpsc::channel();
    let now = Instant::now();
    let old = process(10, 1);
    let replacement = process(10, 2);
    let unaffected = process(20, 1);
    let mut lifecycle = Lifecycle::new(FakeBackend::new(vec![old, unaffected]));
    let dropped = lifecycle.backend.dropped.clone();
    step(&mut lifecycle, &stop, now)?;
    let original_session = activity(&lifecycle, old)?;
    let retained_session = activity(&lifecycle, unaffected)?;

    lifecycle.backend.live.remove(&old);
    lifecycle.backend.live.insert(replacement);
    lifecycle.backend.candidates = vec![replacement, unaffected];
    step(&mut lifecycle, &stop, now + DISCOVERY_INTERVAL)?;
    assert_eq!(*dropped.borrow(), [old]);
    assert!(!lifecycle.targets.contains_key(&old));
    assert_ne!(activity(&lifecycle, replacement)?, original_session);
    assert_eq!(activity(&lifecycle, unaffected)?, retained_session);

    lifecycle.backend.live.remove(&replacement);
    lifecycle.backend.candidates = vec![unaffected];
    step(&mut lifecycle, &stop, now + DISCOVERY_INTERVAL * 2)?;
    assert_eq!(lifecycle.targets.len(), 1);
    assert_eq!(activity(&lifecycle, unaffected)?, retained_session);
    drop(lifecycle);
    assert_eq!(*dropped.borrow(), [old, replacement, unaffected]);
    Ok(())
}

#[test]
fn failed_attachment_retries_without_delaying_other_targets() -> anyhow::Result<()> {
    let (_stop_tx, stop) = mpsc::channel();
    let now = Instant::now();
    let failed = process(10, 1);
    let healthy = process(20, 1);
    let arrival = process(30, 1);
    let mut backend = FakeBackend::new(vec![failed, healthy]);
    backend.attachment_failures.insert(failed);
    let mut lifecycle = Lifecycle::new(backend);
    step(&mut lifecycle, &stop, now)?;
    assert!(matches!(
        activity(&lifecycle, failed)?,
        Activity::Retrying { .. }
    ));
    let healthy_session = activity(&lifecycle, healthy)?;
    assert!(matches!(healthy_session, Activity::Observing { .. }));

    lifecycle.backend.candidates.push(arrival);
    lifecycle.backend.live.insert(arrival);
    step(&mut lifecycle, &stop, now + DISCOVERY_INTERVAL)?;
    assert_eq!(lifecycle.backend.attempts, [failed, healthy, arrival]);

    lifecycle.backend.attachment_failures.clear();
    step(&mut lifecycle, &stop, now + RETRY_INTERVAL)?;
    assert_eq!(
        lifecycle.backend.attempts,
        [failed, healthy, arrival, failed]
    );
    assert!(matches!(
        activity(&lifecycle, failed)?,
        Activity::Observing { .. }
    ));
    assert_eq!(activity(&lifecycle, healthy)?, healthy_session);
    Ok(())
}

#[test]
fn retired_attachment_is_dropped_and_retries_as_a_fresh_session() -> anyhow::Result<()> {
    let (_stop_tx, stop) = mpsc::channel();
    let now = Instant::now();
    let failed = process(10, 1);
    let healthy = process(20, 1);
    let mut lifecycle = Lifecycle::new(FakeBackend::new(vec![failed, healthy]));
    step(&mut lifecycle, &stop, now)?;
    let original = activity(&lifecycle, failed)?;
    let retained = activity(&lifecycle, healthy)?;

    lifecycle
        .targets
        .get_mut(&failed)
        .context("target missing")?
        .retry("verification failed".into(), now);
    assert_eq!(*lifecycle.backend.dropped.borrow(), [failed]);
    assert!(matches!(
        activity(&lifecycle, failed)?,
        Activity::Retrying { .. }
    ));
    step(&mut lifecycle, &stop, now + DISCOVERY_INTERVAL)?;
    assert_eq!(lifecycle.backend.attempts, [failed, healthy]);

    step(&mut lifecycle, &stop, now + RETRY_INTERVAL)?;
    assert_eq!(lifecycle.backend.attempts, [failed, healthy, failed]);
    assert!(matches!(
        activity(&lifecycle, failed)?,
        Activity::Observing { .. }
    ));
    assert_ne!(activity(&lifecycle, failed)?, original);
    assert_eq!(activity(&lifecycle, healthy)?, retained);
    Ok(())
}

#[test]
fn discovery_failures_keep_live_sessions_and_do_not_delay_exit_cleanup() -> anyhow::Result<()> {
    let (_stop_tx, stop) = mpsc::channel();
    let now = Instant::now();
    let first = process(10, 1);
    let second = process(20, 1);
    let mut lifecycle = Lifecycle::new(FakeBackend::new(vec![first, second]));
    step(&mut lifecycle, &stop, now)?;
    let retained = activity(&lifecycle, second)?;

    lifecycle.backend.discovery_fails = true;
    step(&mut lifecycle, &stop, now + DISCOVERY_INTERVAL)?;
    assert!(lifecycle.status().discovery_error.is_some());
    assert_eq!(lifecycle.targets.len(), 2);

    lifecycle.backend.live.remove(&first);
    step(&mut lifecycle, &stop, now + DISCOVERY_INTERVAL * 2)?;
    assert_eq!(lifecycle.backend.discoveries, 2);
    assert!(!lifecycle.targets.contains_key(&first));
    assert_eq!(activity(&lifecycle, second)?, retained);

    lifecycle.backend.discovery_fails = false;
    lifecycle.backend.candidates.clear();
    step(
        &mut lifecycle,
        &stop,
        now + DISCOVERY_INTERVAL + RETRY_INTERVAL,
    )?;
    assert!(lifecycle.status().discovery_error.is_none());
    assert_eq!(
        activity(&lifecycle, second)?,
        retained,
        "a missed enumeration must not evict a live session"
    );
    Ok(())
}

#[test]
fn stop_is_honored_while_waiting_discovering_or_tracking_targets() -> anyhow::Result<()> {
    for state in [
        "waiting",
        "discovery_error",
        "attaching",
        "observing",
        "retrying",
    ] {
        let (stop_tx, stop) = mpsc::channel();
        let first = process(10, 1);
        let mut backend = FakeBackend::new(vec![first, process(20, 1)]);
        backend.discovery_fails = state == "discovery_error";
        if state == "retrying" {
            backend.attachment_failures.insert(first);
        }
        let dropped = backend.dropped.clone();
        let mut lifecycle = Lifecycle::new(backend);
        let mut reached = false;
        let mut publish = |status: HostStatus| {
            let matches = match state {
                "waiting" => status.targets.is_empty() && status.discovery_error.is_none(),
                "discovery_error" => status.discovery_error.is_some(),
                "attaching" => status
                    .targets
                    .iter()
                    .any(|target| matches!(target.activity, Activity::Attaching)),
                "observing" => status
                    .targets
                    .iter()
                    .any(|target| matches!(target.activity, Activity::Observing { .. })),
                _ => status
                    .targets
                    .iter()
                    .any(|target| matches!(target.activity, Activity::Retrying { .. })),
            };
            if matches {
                reached = true;
                stop_tx.send(())?;
            }
            Ok(())
        };
        if lifecycle
            .step(&mut publish, &stop, &Instant::now)?
            .is_some()
        {
            assert_eq!(lifecycle.step(&mut publish, &stop, &Instant::now)?, None);
        }
        assert!(reached, "never reached {state}");
        if state == "attaching" || state == "waiting" {
            assert!(lifecycle.backend.attempts.is_empty());
        }
        drop(lifecycle);
        assert_eq!(dropped.borrow().len(), usize::from(state == "observing"));
    }
    Ok(())
}

#[test]
fn stop_during_attachment_discards_late_results_and_releases_all_sessions() -> anyhow::Result<()> {
    let (stop_tx, stop) = mpsc::channel();
    let now = Instant::now();
    let healthy = process(10, 1);
    let late = process(20, 1);
    let unattempted = process(30, 1);
    let mut lifecycle = Lifecycle::new(FakeBackend::new(vec![healthy]));
    step(&mut lifecycle, &stop, now)?;
    lifecycle.backend.candidates.extend([late, unattempted]);
    lifecycle.backend.live.extend([late, unattempted]);
    lifecycle.backend.during_attach = Some(Box::new(move || {
        let _ = stop_tx.send(());
    }));
    let dropped = lifecycle.backend.dropped.clone();

    assert_eq!(
        lifecycle.step(&mut |_| Ok(()), &stop, &|| now + DISCOVERY_INTERVAL)?,
        None
    );
    assert_eq!(lifecycle.backend.attempts, [healthy, late]);
    assert!(matches!(activity(&lifecycle, late)?, Activity::Attaching));
    assert_eq!(*dropped.borrow(), [late]);
    drop(lifecycle);
    assert_eq!(*dropped.borrow(), [late, healthy]);
    Ok(())
}
