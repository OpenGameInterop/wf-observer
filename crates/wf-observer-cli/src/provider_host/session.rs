use std::time::Duration;

use memory_reader::{AccessError, ProcessMemory};
use provider_sdk::{PollContext, PollResult, Provider, ProviderManifest, ProviderSession};
use tokio::time::Instant;

use crate::service::{POLL_GRACE, PollBatch, PollTicket, ServiceState};

const MIN_POLL: Duration = Duration::from_millis(25);

pub(crate) struct HostedSession<M: ProcessMemory> {
    pub(crate) memory: M,
    provider: Box<dyn ProviderSession>,
    manifest: &'static ProviderManifest,
    started: Instant,
    last_demand: Option<PollTicket>,
    generations: Option<PollTicket>,
    next_poll: Option<Instant>,
}

impl<M: ProcessMemory> HostedSession<M> {
    #[hotpath::measure]
    pub(crate) fn start(provider: &'static dyn Provider, mut memory: M) -> anyhow::Result<Self> {
        let target = memory.target().clone();
        memory.verify()?;
        let session = provider.start(&target, &mut memory);
        memory.verify()?;
        Ok(Self {
            memory,
            provider: session?,
            manifest: provider.manifest(),
            started: Instant::now(),
            last_demand: None,
            generations: None,
            next_poll: None,
        })
    }

    /// Verification failure is fatal to this attachment: the lifecycle owner
    /// must drop the host and its provider caches, then rediscover/retry.
    #[hotpath::measure]
    pub(crate) fn poll(
        &mut self,
        state: &ServiceState,
        session_id: &str,
        now: Instant,
    ) -> Result<Option<Duration>, AccessError> {
        let Some(ticket) = state.ticket(session_id) else {
            return Ok(None);
        };
        let changed = self
            .last_demand
            .as_ref()
            .is_none_or(|previous| !ticket.same_demand(previous));
        if !changed && self.next_poll.is_none_or(|deadline| now < deadline) {
            return Ok(self
                .next_poll
                .map(|deadline| deadline.saturating_duration_since(now)));
        }
        let demand: Vec<_> =
            self.manifest
                .capabilities
                .iter()
                .filter(|cap| {
                    ticket.topics.iter().any(|d| {
                        d.key.topic == cap.topic && d.key.schema_version == cap.schema_version
                    })
                })
                .collect();
        let batch = PollBatch::new(self.manifest);
        self.memory
            .verify()
            .inspect_err(|_| state.fail_poll(&ticket))?;
        for cap in &demand {
            if let Some(key) = ticket
                .topics
                .iter()
                .find(|d| d.key.topic == cap.topic && d.key.schema_version == cap.schema_version)
                && self
                    .generations
                    .as_ref()
                    .is_none_or(|previous| !ticket.same_generation(previous, &key.key))
            {
                self.provider.begin_generation(cap);
            }
        }
        self.generations = Some(ticket.clone());
        let result = (|| {
            let mut events = batch.events();
            let mut health = batch.health();
            let schedule = self.provider.poll(&mut PollContext {
                memory: &mut self.memory,
                events: &mut events,
                health: &mut health,
                demand: &demand,
                now: now.saturating_duration_since(self.started),
            })?;
            batch.game_build(self.provider.game_build())?;
            Ok::<_, provider_sdk::ProviderError>(schedule)
        })();
        // Check even when the provider returned an error or only emitted health:
        // it may have changed caches before the target exited or its PID was reused.
        if let Err(error) = self.memory.verify() {
            self.provider.poll_completed(false);
            state.fail_poll(&ticket);
            return Err(error);
        }
        let resets = batch.reset_intents();
        self.last_demand = Some(ticket.clone());
        let end = Instant::now().max(now);
        let result = result.and_then(|schedule| {
            deadlines(schedule, end).ok_or_else(|| {
                provider_sdk::ProviderError::Failed("poll delay exceeds the clock range".into())
            })
        });
        match result {
            Ok((next_poll, deadline)) => {
                self.next_poll = next_poll;
                match state.commit_poll(&ticket, batch, deadline) {
                    Ok(true) => {
                        for previous in [&mut self.generations, &mut self.last_demand]
                            .into_iter()
                            .flatten()
                        {
                            previous.rebase_resets(&resets);
                        }
                        self.provider.poll_completed(true);
                    }
                    Ok(false) => {
                        self.provider.poll_completed(false);
                        self.last_demand = state.discard_poll(&ticket, &resets, false);
                        self.next_poll = Some(end + Duration::from_secs(1));
                    }
                    Err(error) => {
                        self.provider.poll_completed(false);
                        tracing::warn!(%error, "provider publication rejected");
                        self.last_demand = state.discard_poll(&ticket, &resets, true);
                        self.next_poll = Some(end + Duration::from_secs(1));
                    }
                }
            }
            Err(error) => {
                self.provider.poll_completed(false);
                tracing::warn!(%error, provider = self.manifest.id, "provider poll failed");
                self.last_demand = state.discard_poll(&ticket, &resets, true);
                self.next_poll = Some(end + Duration::from_secs(1));
            }
        }
        Ok(self
            .next_poll
            .map(|deadline| deadline.saturating_duration_since(end)))
    }
}

fn deadlines(schedule: PollResult, end: Instant) -> Option<(Option<Instant>, Instant)> {
    let next_poll = match schedule {
        PollResult::After(delay) => Some(end.checked_add(delay.max(MIN_POLL))?),
        PollResult::Idle => None,
    };
    Some((next_poll, next_poll.unwrap_or(end).checked_add(POLL_GRACE)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support as fixture;
    use parking_lot::Mutex;
    use provider_sdk::CapabilityDescriptor;
    use std::sync::Arc;

    struct CursorProbe {
        state: ServiceState,
        log: Arc<Mutex<Vec<String>>>,
        schedule: PollResult,
    }

    impl ProviderSession for CursorProbe {
        fn begin_generation(&mut self, cap: &CapabilityDescriptor) {
            self.log.lock().push(cap.topic.into());
        }
        fn poll_completed(&mut self, committed: bool) {
            self.log.lock().push(committed.to_string());
        }
        fn poll(
            &mut self,
            context: &mut PollContext<'_>,
        ) -> Result<PollResult, provider_sdk::ProviderError> {
            let mut bytes = [0; 4];
            context.memory.read_into(0x1000, &mut bytes)?;
            let command = u32::from_le_bytes(bytes);
            for cap in context.demand {
                if command == 8 {
                    context.events.reset(cap)?;
                }
                context.events.snapshot(cap, &serde_json::json!(7))?;
            }
            match command {
                9 => self.state.expire(Instant::now() + Duration::from_secs(60)),
                10 => return Err(provider_sdk::ProviderError::Failed("test rejection".into())),
                11 => {
                    let mut undeclared = fixture::CAPS[0];
                    undeclared.topic = "undeclared";
                    let _ = context.events.snapshot(&undeclared, &serde_json::json!(7));
                }
                _ => {}
            }
            Ok(self.schedule)
        }
    }

    #[test]
    fn cursors_follow_commits_and_topic_generations_not_poll_epochs() -> anyhow::Result<()> {
        let state = fixture::state()?;
        fixture::observe(&state, &["one"])?;
        let first = state.view().subscribe(fixture::selection())?;
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut host = HostedSession::start(&fixture::PROVIDER, fixture::FixtureMemory::new()?)?;
        host.provider = Box::new(CursorProbe {
            state: state.clone(),
            log: log.clone(),
            schedule: PollResult::After(Duration::from_millis(50)),
        });
        let mut now = Instant::now();
        host.poll(&state, "one", now)?;
        assert_eq!(*log.lock(), ["fixture.count", "true"]);
        log.lock().clear();

        // An accepted source reset already belongs to the provider's new cursor.
        host.memory.value = 8;
        now += Duration::from_secs(2);
        host.poll(&state, "one", now)?;
        host.memory.value = 7;
        now += Duration::from_secs(2);
        host.poll(&state, "one", now)?;
        assert_eq!(*log.lock(), ["true", "true"]);
        log.lock().clear();

        // Stale epochs, provider errors and ignored sink errors must all reject.
        for command in [9, 10, 11] {
            host.memory.value = command;
            now += Duration::from_secs(2);
            host.poll(&state, "one", now)?;
            host.memory.value = 7;
            let reads = host.memory.reads;
            host.poll(&state, "one", now + Duration::from_millis(500))?;
            assert_eq!(host.memory.reads, reads, "rejected polls must honor retry");
            now += Duration::from_secs(2);
            host.poll(&state, "one", now)?;
            assert_eq!(*log.lock(), ["false", "true"]);
            log.lock().clear();
        }

        let mut shared = fixture::selection();
        shared.topics = vec![fixture::topic(1)];
        let _shared = state.view().subscribe(shared)?;
        now += Duration::from_secs(2);
        host.poll(&state, "one", now)?;
        assert_eq!(*log.lock(), ["fixture.shared", "true"]);
        log.lock().clear();

        // Stop/resume between polls must baseline this topic, even with other demand.
        drop(first);
        let _resumed = state.view().subscribe(fixture::selection())?;
        now += Duration::from_secs(2);
        host.poll(&state, "one", now)?;
        assert_eq!(*log.lock(), ["fixture.count", "true"]);
        state.shutdown();
        Ok(())
    }

    #[test]
    fn requested_delays_and_idle_ignore_freshness_epochs() -> anyhow::Result<()> {
        use anyhow::Context as _;

        let state = fixture::state()?;
        fixture::observe(&state, &["one"])?;
        let subscription = state.view().subscribe(fixture::selection())?;
        let request = fixture::request(&state, "one")?;
        let mut host = HostedSession::start(&fixture::PROVIDER, fixture::FixtureMemory::new()?)?;
        host.provider = Box::new(CursorProbe {
            state: state.clone(),
            log: Arc::default(),
            schedule: PollResult::After(Duration::from_secs(60)),
        });
        host.memory.value = 8;
        let now = Instant::now();
        host.poll(&state, "one", now)?;
        let due = host.next_poll.context("missing next poll")?;
        assert!(due >= now + Duration::from_secs(60));
        state.expire(due - Duration::from_secs(1));
        assert!(state.view().snapshot(&request).is_ok());
        host.poll(&state, "one", due - Duration::from_secs(1))?;
        assert_eq!(host.memory.reads, 1);
        host.poll(&state, "one", due)?;
        assert_eq!(host.memory.reads, 2);

        host.provider = Box::new(CursorProbe {
            state: state.clone(),
            log: Arc::default(),
            schedule: PollResult::Idle,
        });
        let idle = host.next_poll.context("missing next poll")?;
        assert!(host.poll(&state, "one", idle)?.is_none());
        assert_eq!(host.memory.reads, 3);
        state.expire(idle + POLL_GRACE + Duration::from_secs(1));
        assert!(state.view().snapshot(&request).is_err());
        assert!(
            host.poll(&state, "one", idle + Duration::from_secs(60))?
                .is_none()
        );
        assert_eq!(
            host.memory.reads, 3,
            "expiry must not wake an idle provider"
        );

        // Stop/resume between calls changes the generation, even with the same topic set.
        drop(subscription);
        let _resumed = state.view().subscribe(fixture::selection())?;
        host.poll(&state, "one", idle + Duration::from_secs(60))?;
        assert_eq!(host.memory.reads, 4);

        assert_eq!(
            deadlines(PollResult::After(Duration::ZERO), now),
            Some((Some(now + MIN_POLL), now + MIN_POLL + POLL_GRACE))
        );
        assert!(deadlines(PollResult::After(Duration::MAX), now).is_none());
        Ok(())
    }
}
