use super::HostedSession;
use crate::{service::ServiceState, test_support as fixture};
use anyhow::Context as _;
use memory_reader::{AccessError, MemoryModule, ProcessMemory, ProcessMetadata, Target};
use protocol::v1 as wire;
use provider_sdk::{
    PollContext, PollResult, Provider, ProviderError, ProviderManifest, ProviderSession,
};
use std::time::Duration;
use tokio::time::Instant;

struct PausedMemory {
    memory: fixture::FixtureMemory,
    entered: std::sync::mpsc::SyncSender<()>,
    resume: std::sync::mpsc::Receiver<()>,
}

/// Identity changes independently of read success, as with a PID-based reader.
struct ExpiringMemory {
    memory: fixture::FixtureMemory,
    current: bool,
    expire_on_read: bool,
    expire_state: Option<(ServiceState, Instant)>,
}

#[derive(Debug, ..Copy)]
enum AfterReset {
    Idle,
    Error,
    Rejected,
}

struct AccountProvider(AfterReset);
static ACCOUNT_PROVIDER: AccountProvider = AccountProvider(AfterReset::Idle);
static ERROR_PROVIDER: AccountProvider = AccountProvider(AfterReset::Error);
static REJECTED_PROVIDER: AccountProvider = AccountProvider(AfterReset::Rejected);

impl Provider for AccountProvider {
    fn manifest(&self) -> &'static ProviderManifest {
        &fixture::MANIFEST
    }
    fn identify_process(&self, process: &ProcessMetadata<'_>) -> Option<&'static str> {
        fixture::PROVIDER.identify_process(process)
    }
    fn start(
        &self,
        _: &Target,
        _: &mut dyn ProcessMemory,
    ) -> Result<Box<dyn ProviderSession>, ProviderError> {
        Ok(Box::new(AccountSession {
            account: None,
            after_reset: self.0,
        }))
    }
}

struct AccountSession {
    account: Option<u32>,
    after_reset: AfterReset,
}

impl ProviderSession for AccountSession {
    fn poll(&mut self, context: &mut PollContext<'_>) -> Result<PollResult, ProviderError> {
        let mut bytes = [0; 4];
        context.memory.read_into(0x1000, &mut bytes)?;
        let account = u32::from_le_bytes(bytes);
        let changed = self.account.is_some_and(|previous| previous != account);
        let cap = &fixture::CAPS[0];
        if changed {
            context.events.reset(cap)?;
        }
        self.account = Some(account);
        if changed {
            match self.after_reset {
                AfterReset::Error => return Err(ProviderError::Failed("after reset".into())),
                AfterReset::Rejected => {
                    // The provider ignores the sink error; commit must still reject this batch.
                    let mut undeclared = *cap;
                    undeclared.topic = "undeclared";
                    let _ = context
                        .events
                        .snapshot(&undeclared, &serde_json::json!(account));
                    return Ok(PollResult::After(Duration::from_millis(50)));
                }
                AfterReset::Idle => {}
            }
        }
        let value = serde_json::json!({"account": account});
        context.events.snapshot(cap, &value)?;
        context.events.event(cap, &value)?;
        Ok(if changed {
            PollResult::Idle
        } else {
            PollResult::After(Duration::from_millis(50))
        })
    }
}

impl ProcessMemory for ExpiringMemory {
    fn target(&self) -> &Target {
        self.memory.target()
    }
    fn verify(&mut self) -> Result<(), AccessError> {
        if self.current {
            Ok(())
        } else {
            Err(AccessError::TargetChanged(self.target().instance().pid()))
        }
    }
    fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
        self.memory.modules()
    }
    fn read_into(&mut self, address: u64, buffer: &mut [u8]) -> Result<(), AccessError> {
        let result = self.memory.read_into(address, buffer);
        self.current &= !self.expire_on_read;
        if let Some((state, now)) = self.expire_state.take() {
            state.expire(now);
        }
        result
    }
}

#[tokio::test]
async fn identity_change_rejects_poll_output_even_when_the_provider_errors() -> anyhow::Result<()> {
    for (expires_before_poll, provider_errors) in [(true, false), (false, false), (false, true)] {
        let state = fixture::state()?;
        fixture::observe(&state, &["one"])?;
        let mut sub = state.view().subscribe(fixture::selection())?;
        let request = fixture::request(&state, "one")?;
        let mut host = HostedSession::start(
            if provider_errors {
                &ERROR_PROVIDER
            } else {
                &ACCOUNT_PROVIDER
            },
            ExpiringMemory {
                memory: fixture::FixtureMemory::new()?,
                current: true,
                expire_on_read: false,
                expire_state: None,
            },
        )?;
        let now = Instant::now();
        host.poll(&state, "one", now)?;
        assert!(state.view().snapshot(&request).is_ok());
        // Drain the accepted sample so any later publication is distinguishable.
        loop {
            let item = tokio::time::timeout(Duration::from_secs(1), sub.next())
                .await?
                .context("subscription ended before the accepted event")?;
            if matches!(
                item,
                wire::SubscriptionItem::Update(wire::UpdateEnvelope {
                    update: wire::SubscriptionUpdate::Event(_),
                    ..
                })
            ) {
                break;
            }
        }

        let reads = host.memory.memory.reads;
        host.memory.current = !expires_before_poll;
        host.memory.expire_on_read = true;
        host.memory.memory.value += 1;
        assert!(matches!(
            host.poll(&state, "one", now + Duration::from_secs(1)),
            Err(AccessError::TargetChanged(_))
        ));
        assert_eq!(
            host.memory.memory.reads - reads,
            usize::from(!expires_before_poll)
        );
        assert!(matches!(
            state.view().snapshot(&request),
            Err(wire::RequestError::Unavailable { .. })
        ));
        // The only queued update must be failed health, not the staged reset or data.
        let item = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await?
            .context("missing failed health")?;
        assert!(matches!(
            item,
            wire::SubscriptionItem::Update(wire::UpdateEnvelope {
                update: wire::SubscriptionUpdate::TopicChanged(wire::TopicSnapshot {
                    health: wire::CapabilityHealth::Unavailable { .. },
                    snapshot: None,
                    ..
                }),
                ..
            })
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), sub.next())
                .await
                .is_err()
        );
        state.shutdown();
    }
    Ok(())
}

#[tokio::test]
async fn account_resets_survive_rejected_polls_without_bypassing_retry() -> anyhow::Result<()> {
    for provider in [&ERROR_PROVIDER, &REJECTED_PROVIDER, &ACCOUNT_PROVIDER] {
        let state = fixture::state()?;
        fixture::observe(&state, &["one"])?;
        let mut sub = state.view().subscribe(fixture::selection())?;
        let request = fixture::request(&state, "one")?;
        let mut host = HostedSession::start(
            provider,
            ExpiringMemory {
                memory: fixture::FixtureMemory::new()?,
                current: true,
                expire_on_read: false,
                expire_state: None,
            },
        )?;
        let now = Instant::now();
        host.poll(&state, "one", now)?;
        let initial = state.view().snapshot(&request)?;
        host.memory.memory.value += 1;
        if matches!(provider.0, AfterReset::Idle) {
            host.memory.expire_state = Some((state.clone(), now + Duration::from_secs(60)));
        }
        let rejected = now + Duration::from_secs(1);
        let retry = host
            .poll(&state, "one", rejected)?
            .context("missing retry")?;
        assert!(matches!(
            state.view().snapshot(&request),
            Err(wire::RequestError::Unavailable { .. })
        ));
        assert!(retry >= Duration::from_millis(25));
        let reads = host.memory.memory.reads;
        host.poll(&state, "one", rejected + retry / 2)?;
        assert_eq!(
            host.memory.memory.reads, reads,
            "{:?} bypassed retry",
            provider.0
        );
        host.poll(&state, "one", rejected + retry)?;
        let replacement = state.view().snapshot(&request)?;
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(replacement.payload.as_str())?,
            serde_json::json!({"account": 8})
        );
        assert!(
            replacement.metadata.generation > initial.metadata.generation,
            "{:?} lost reset",
            provider.0
        );
        let mut resets = 0;
        let mut events = 0;
        loop {
            let item = tokio::time::timeout(Duration::from_secs(1), sub.next())
                .await?
                .context("missing account update")?;
            if let wire::SubscriptionItem::Update(wire::UpdateEnvelope { update, .. }) = item {
                match update {
                    wire::SubscriptionUpdate::TopicReset {
                        reason: wire::ResetReason::SourceChanged,
                        ..
                    } => resets += 1,
                    wire::SubscriptionUpdate::Event(event) => {
                        events += 1;
                        if event.metadata.sequence > replacement.metadata.sequence {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(events, 2, "{:?} event count", provider.0);
        assert_eq!(resets, 1, "{:?} reset count", provider.0);
        state.shutdown();
    }
    Ok(())
}

impl ProcessMemory for PausedMemory {
    fn target(&self) -> &Target {
        self.memory.target()
    }
    fn verify(&mut self) -> Result<(), AccessError> {
        self.memory.verify()
    }
    fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
        self.memory.modules()
    }
    fn read_into(&mut self, address: u64, buffer: &mut [u8]) -> Result<(), AccessError> {
        if self.entered.send(()).is_err()
            || self.resume.recv_timeout(Duration::from_secs(5)).is_err()
        {
            return Err(AccessError::InvalidReadRange {
                address,
                length: buffer.len(),
            });
        }
        self.memory.read_into(address, buffer)
    }
}

#[test]
fn shutdown_fences_native_work_without_waiting_for_its_completion() -> anyhow::Result<()> {
    let state = fixture::state()?;
    fixture::observe(&state, &["one"])?;
    let _sub = state.view().subscribe(fixture::selection())?;
    let request = fixture::request(&state, "one")?;
    let (entered, waiting) = std::sync::mpsc::sync_channel(1);
    let (resume, paused) = std::sync::mpsc::sync_channel(1);
    let memory = PausedMemory {
        memory: fixture::FixtureMemory::new()?,
        entered,
        resume: paused,
    };
    let mut host = HostedSession::start(&fixture::PROVIDER, memory)?;
    let working = state.clone();
    let worker = std::thread::spawn(move || host.poll(&working, "one", Instant::now()));
    waiting.recv_timeout(Duration::from_secs(5))?;
    state.shutdown();
    assert_eq!(
        state.view().snapshot(&request),
        Err(wire::RequestError::Idle)
    );
    resume.send(())?;
    worker
        .join()
        .map_err(|_| anyhow::anyhow!("provider worker panicked"))??;
    assert_eq!(
        state.view().snapshot(&request),
        Err(wire::RequestError::Idle)
    );
    Ok(())
}

#[test]
fn shared_sources_poll_once_per_provider_not_once_per_subscriber() -> anyhow::Result<()> {
    let state = fixture::state()?;
    fixture::observe(&state, &["one"])?;
    let mut host = HostedSession::start(&fixture::PROVIDER, fixture::FixtureMemory::new()?)?;
    let now = Instant::now();
    host.poll(&state, "one", now)?;
    assert_eq!(host.memory.reads, 0);
    let mut selection = fixture::selection();
    selection.topics.push(fixture::topic(1));
    let first = state.view().subscribe(selection.clone())?;
    let second = state.view().subscribe(selection)?;
    host.poll(&state, "one", now)?;
    assert_eq!(host.memory.reads, 1);
    assert!(
        state
            .view()
            .snapshot(&fixture::request(&state, "one")?)
            .is_ok()
    );
    host.poll(&state, "one", now)?;
    assert_eq!(host.memory.reads, 1);
    drop(first);
    host.poll(&state, "one", now + Duration::from_millis(100))?;
    assert_eq!(host.memory.reads, 2);
    drop(second);
    host.poll(&state, "one", now + Duration::from_millis(200))?;
    assert_eq!(host.memory.reads, 2);
    let _resumed = state.view().subscribe(fixture::selection())?;
    host.memory.failed = true;
    host.poll(&state, "one", now + Duration::from_millis(300))?;
    assert!(matches!(
        state.view().snapshot(&fixture::request(&state, "one")?),
        Err(wire::RequestError::Unavailable { .. })
    ));
    state.shutdown();
    assert!(state.ticket("one").is_none());
    assert!(host.poll(&state, "one", now)?.is_none());
    Ok(())
}
