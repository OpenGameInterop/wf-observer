//! Synchronous session management, discovery scheduling, and cancellation.

use std::{collections::BTreeMap, sync::mpsc, time::Duration};

use tokio::time::Instant;

use crate::runtime::{Activity, HostStatus, RecordedProcess, Registration, TargetStatus};
use crate::service::ServiceState;

use super::backend::{Backend, NativeBackend};

pub(super) const DISCOVERY_INTERVAL: Duration = Duration::from_secs(1);
pub(super) const RETRY_INTERVAL: Duration = Duration::from_secs(5);

/// Runs until cancellation, keeping native process work off the async executor.
pub(crate) fn run(
    registration: &mut Registration,
    stop: &mpsc::Receiver<()>,
    state: &ServiceState,
) -> anyhow::Result<()> {
    let mut lifecycle = Lifecycle::new(NativeBackend);
    let mut publish = |status: HostStatus| {
        state.update_lifecycle(status.clone())?;
        registration.update(status)
    };

    loop {
        let revision = state.revision();
        let Some(mut delay) = lifecycle.step(&mut publish, stop, &Instant::now)? else {
            break;
        };
        let mut retired = false;
        for target in lifecycle.targets.values_mut() {
            if stopping(stop) {
                return Ok(());
            }
            if let (Some(attachment), Activity::Observing { session_id }) =
                (&mut target.attachment, &target.status.activity)
            {
                match attachment.poll(state, session_id, Instant::now()) {
                    Ok(Some(poll_delay)) => delay = delay.min(poll_delay),
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(%error, "retiring unverified provider attachment");
                        target.retry(error.to_string(), Instant::now());
                        retired = true;
                    }
                }
            }
        }
        if retired {
            publish(lifecycle.status())?;
        }
        state.wait(revision, delay);
    }
    // All retained attachments are dropped before the caller unregisters the service.
    Ok(())
}

pub(super) struct Lifecycle<B: Backend> {
    pub(super) backend: B,
    pub(super) targets: BTreeMap<RecordedProcess, TrackedTarget<B::Attachment>>,
    discovery_error: Option<String>,
    next_discovery: Option<Instant>,
}

pub(super) struct TrackedTarget<A> {
    pub(super) status: TargetStatus,
    attachment: Option<A>,
    retry_at: Option<Instant>,
}

impl<A> TrackedTarget<A> {
    pub(super) fn retry(&mut self, error: String, now: Instant) {
        self.attachment = None;
        self.status.activity = Activity::Retrying { error };
        self.retry_at = Some(now + RETRY_INTERVAL);
    }
}

impl<B: Backend> Lifecycle<B> {
    pub(super) const fn new(backend: B) -> Self {
        Self {
            backend,
            targets: BTreeMap::new(),
            discovery_error: None,
            next_discovery: None,
        }
    }

    /// Enumerates once when due, then attempts each eligible process at most once.
    /// `None` means stop. The clock is injectable so retry tests need no sleeps.
    pub(super) fn step(
        &mut self,
        publish: &mut impl FnMut(HostStatus) -> anyhow::Result<()>,
        stop: &mpsc::Receiver<()>,
        now: &impl Fn() -> Instant,
    ) -> anyhow::Result<Option<Duration>> {
        if stopping(stop) {
            return Ok(None);
        }

        self.targets.retain(|_, target| {
            target
                .attachment
                .as_ref()
                .is_none_or(|attachment| self.backend.is_current(attachment))
        });
        publish(self.status())?;
        if stopping(stop) {
            return Ok(None);
        }

        if self.next_discovery.is_some_and(|deadline| now() < deadline) {
            return Ok(Some(DISCOVERY_INTERVAL));
        }

        let discovered = self.backend.discover();
        if stopping(stop) {
            return Ok(None);
        }
        let candidates = match discovered {
            Ok(candidates) => candidates,
            Err(error) => {
                self.discovery_error = Some(format!("{error:#}"));
                self.next_discovery = Some(now() + RETRY_INTERVAL);
                publish(self.status())?;
                return Ok(Some(DISCOVERY_INTERVAL));
            }
        };
        self.discovery_error = None;
        self.next_discovery = Some(now() + DISCOVERY_INTERVAL);
        let candidates: BTreeMap<_, _> = candidates
            .into_iter()
            .map(|candidate| (B::target(&candidate).process, candidate))
            .collect();

        // A missed enumeration must not evict a verified live attachment. Failed
        // attempts, however, need not be retained once discovery stops listing them.
        self.targets.retain(|process, target| {
            target.attachment.is_some() || candidates.contains_key(process)
        });
        publish(self.status())?;

        for (process, candidate) in candidates {
            if stopping(stop) {
                return Ok(None);
            }
            if self.targets.get(&process).is_some_and(|target| {
                target.attachment.is_some()
                    || target.retry_at.is_some_and(|deadline| now() < deadline)
            }) {
                continue;
            }

            let target = B::target(&candidate);
            self.targets.insert(
                process,
                TrackedTarget {
                    status: TargetStatus {
                        target: target.clone(),
                        activity: Activity::Attaching,
                    },
                    attachment: None,
                    retry_at: None,
                },
            );
            publish(self.status())?;
            if stopping(stop) {
                return Ok(None);
            }

            // Native calls cannot be cancelled mid-call. Discard a late result on
            // shutdown; the service joins this worker before releasing its lock.
            let attached = self.attach(&candidate);
            if stopping(stop) {
                return Ok(None);
            }
            let tracked = match attached {
                Ok((attachment, activity)) => TrackedTarget {
                    status: TargetStatus { target, activity },
                    attachment: Some(attachment),
                    retry_at: None,
                },
                Err(error) => TrackedTarget {
                    status: TargetStatus {
                        target,
                        activity: Activity::Retrying {
                            error: format!("{error:#}"),
                        },
                    },
                    attachment: None,
                    retry_at: Some(now() + RETRY_INTERVAL),
                },
            };
            self.targets.insert(process, tracked);
            publish(self.status())?;
        }
        Ok(Some(DISCOVERY_INTERVAL))
    }

    fn attach(&mut self, candidate: &B::Candidate) -> anyhow::Result<(B::Attachment, Activity)> {
        let attachment = self.backend.attach(candidate)?;
        anyhow::ensure!(
            self.backend.is_current(&attachment),
            "target exited during attachment"
        );
        Ok((attachment, Activity::observing()?))
    }

    pub(super) fn status(&self) -> HostStatus {
        HostStatus {
            discovery_error: self.discovery_error.clone(),
            targets: self
                .targets
                .values()
                .map(|target| target.status.clone())
                .collect(),
        }
    }
}

fn stopping(stop: &mpsc::Receiver<()>) -> bool {
    !matches!(stop.try_recv(), Err(mpsc::TryRecvError::Empty))
}
