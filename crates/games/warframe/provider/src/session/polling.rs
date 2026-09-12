use super::validation::{CachedCheck, Executable, Retry, identify};
use crate::{
    item_type::{self, ItemTypeCache, facts::ITEM_TYPES},
    roots::{self, LoginIdentity, LoginResolution},
    string_pool::{self, StringTokenCache, facts::STRINGS},
    target::READ_LIMITS,
    topics,
};
use memory_reader::ProcessMemory;
use provider_sdk::{
    CapabilityDescriptor, CapabilityHealth, PollContext, PollResult, ProviderError,
    ProviderSession, UnavailableReason, memory::TargetReader,
};
use std::time::Duration;
use warframe_model::InventorySnapshot;

#[cfg(test)]
#[path = "acquisition_tests.rs"]
mod acquisition_tests;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) static INVENTORY: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.inventory",
    schema_version: 1,
    snapshots: true,
    events: false,
};

#[derive(Default)]
pub(crate) struct WarframeSession {
    executable: CachedCheck<Executable>,
    build_name: Option<String>,
    login_layout: CachedCheck,
    string_layout: CachedCheck,
    item_layout: CachedCheck,
    inventory: TopicState,
    login: Option<LoginIdentity>,
    items: ItemTypeCache,
    strings: StringTokenCache,
}

/// Demand controls acquisition, not the lifetime of a successful layout check.
#[derive(Default)]
struct TopicState {
    layout: CachedCheck,
    demanded: bool,
    next_poll: Duration,
}

impl TopicState {
    fn demand(&mut self, demanded: bool, now: Duration) {
        if demanded && !self.demanded {
            self.next_poll = now;
        }
        self.demanded = demanded;
    }

    fn due(&self, now: Duration) -> bool {
        self.demanded && now >= self.next_poll
    }
}

impl ProviderSession for WarframeSession {
    fn begin_generation(&mut self, cap: &CapabilityDescriptor) {
        for (own, topic) in self.account_topics() {
            if own.topic == cap.topic {
                topic.next_poll = Duration::ZERO;
            }
        }
    }

    fn game_build(&self) -> Option<&str> {
        self.build_name.as_deref()
    }

    fn poll(&mut self, context: &mut PollContext<'_>) -> Result<PollResult, ProviderError> {
        for (cap, topic) in self.account_topics() {
            let demanded = context.demand.iter().any(|wanted| {
                wanted.topic == cap.topic && wanted.schema_version == cap.schema_version
            });
            topic.demand(demanded, context.now);
        }
        if !self
            .account_topics()
            .iter()
            .any(|(_, topic)| topic.demanded)
        {
            self.login = None;
            return Ok(PollResult::Idle);
        }
        if self
            .account_topics()
            .iter()
            .any(|(_, topic)| topic.due(context.now))
        {
            let ready = self
                .executable
                .ensure(context.now, || identify(context.memory))
                .and_then(|image| {
                    self.build_name
                        .get_or_insert_with(|| image.actual.to_string());
                    self.login_layout.validate(context.now, "login", || {
                        roots::validate_login_layout(
                            context.memory,
                            image.base,
                            image.actual.image_size,
                        )
                    })?;
                    Ok(image)
                });
            match ready {
                Ok(image) => {
                    self.validate_due(context, image)?;
                    if self
                        .account_topics()
                        .iter()
                        .any(|(_, topic)| topic.due(context.now))
                    {
                        self.acquire(context, image)?;
                    }
                }
                Err(retry) => self.account_unavailable(context, &retry)?,
            }
        }
        Ok(self.schedule(context.now))
    }
}

impl WarframeSession {
    /// These topics share login ownership, but have independent layouts and deadlines.
    fn account_topics(&mut self) -> [(&'static CapabilityDescriptor, &mut TopicState); 1] {
        [(&INVENTORY, &mut self.inventory)]
    }

    fn schedule(&mut self, now: Duration) -> PollResult {
        self.account_topics()
            .into_iter()
            .filter(|(_, topic)| topic.demanded)
            .map(|(_, topic)| topic.next_poll.saturating_sub(now))
            .min()
            .map_or(PollResult::Idle, PollResult::After)
    }

    fn acquire(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
    ) -> Result<(), ProviderError> {
        let before = match roots::resolve_login(context.memory, image.base, image.actual.image_size)
        {
            Ok(LoginResolution::Present(login)) => login,
            Ok(LoginResolution::Absent) => {
                self.clear_login(context)?;
                return self.account_unavailable(
                    context,
                    &Retry {
                        reason: UnavailableReason::TargetNotReady,
                        at: context.now.saturating_add(SAMPLE_INTERVAL),
                    },
                );
            }
            Err(error) => {
                tracing::debug!(%error, "account roots unavailable");
                self.clear_login(context)?;
                return self.account_unavailable(
                    context,
                    &Retry {
                        reason: (&error).into(),
                        at: context.now.saturating_add(SAMPLE_INTERVAL),
                    },
                );
            }
        };
        let mut reset = false;
        if self.login.as_ref().is_some_and(|old| old != &before) {
            self.reset_account(context.events, context.now)?;
            reset = true;
            // Reset also wakes previously deferred topics; their cached failures still apply.
            self.validate_due(context, image)?;
        }
        self.login = Some(before.clone());
        let inventory = self
            .inventory
            .due(context.now)
            .then(|| self.sample_inventory(context, image, &before));
        // Even a failed acquisition must not retain a previous account's data.
        // Results stay local until all account-scoped reads have completed.
        let after = roots::resolve_login(context.memory, image.base, image.actual.image_size);
        if !matches!(&after, Ok(LoginResolution::Present(login)) if login == &before) {
            if !reset {
                self.reset_account(context.events, context.now)?;
            }
            self.login = None;
            return self.account_unavailable(
                context,
                &Retry {
                    reason: UnavailableReason::TargetNotReady,
                    at: context.now.saturating_add(SAMPLE_INTERVAL),
                },
            );
        }
        self.publish(context, inventory)?;
        Ok(())
    }

    /// Called only after the shared ownership recheck; an acquisition error affects only its topic.
    fn publish(
        &mut self,
        context: &mut PollContext<'_>,
        inventory: Option<Result<InventorySnapshot, Retry>>,
    ) -> Result<(), ProviderError> {
        snapshot(context, &INVENTORY, &mut self.inventory, inventory)
    }

    fn sample_inventory(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
        login: &LoginIdentity,
    ) -> Result<InventorySnapshot, Retry> {
        topics::read_inventory(
            context.memory,
            image.base,
            image.actual.image_size,
            login,
            &mut self.items,
            &mut self.strings,
        )
        .map_err(|error| {
            tracing::debug!(%error, "inventory acquisition unavailable");
            Retry {
                reason: (&error).into(),
                at: context.now.saturating_add(SAMPLE_INTERVAL),
            }
        })
    }

    /// Failed validation updates that topic's health; compatible topics remain due for acquisition.
    fn validate_due(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
    ) -> Result<(), ProviderError> {
        if self.inventory.due(context.now) {
            let ready = self
                .validate_item_paths(context.memory, image, context.now)
                .and_then(|()| {
                    self.inventory
                        .layout
                        .validate(context.now, INVENTORY.topic, || {
                            topics::validate_inventory_layout(
                                context.memory,
                                image.base,
                                image.actual.image_size,
                            )
                        })
                });
            if let Err(retry) = ready {
                unavailable(context, &INVENTORY, &mut self.inventory, retry)?;
            }
        }
        Ok(())
    }

    fn validate_item_paths(
        &mut self,
        memory: &mut dyn ProcessMemory,
        image: Executable,
        now: Duration,
    ) -> Result<(), Retry> {
        self.string_layout.validate(now, "string pool", || {
            let mut reader =
                TargetReader::new(memory, image.base, image.actual.image_size, READ_LIMITS)?;
            string_pool::validate_string_pool_layout(&mut reader, STRINGS)
        })?;
        self.item_layout.validate(now, "item types", || {
            let mut reader =
                TargetReader::new(memory, image.base, image.actual.image_size, READ_LIMITS)?;
            item_type::validate_item_types(&mut reader, ITEM_TYPES)
        })
    }

    fn clear_login(&mut self, context: &mut PollContext<'_>) -> Result<(), ProviderError> {
        if self.login.take().is_some() {
            self.reset_account(context.events, context.now)?;
        }
        Ok(())
    }

    fn reset_account(
        &mut self,
        events: &mut dyn provider_sdk::EventSink,
        now: Duration,
    ) -> Result<(), ProviderError> {
        for (cap, topic) in self.account_topics() {
            if topic.demanded {
                events.reset(cap)?;
                topic.next_poll = now;
            }
        }
        Ok(())
    }

    fn account_unavailable(
        &mut self,
        context: &mut PollContext<'_>,
        retry: &Retry,
    ) -> Result<(), ProviderError> {
        for (cap, topic) in self.account_topics() {
            if topic.demanded {
                unavailable(context, cap, topic, retry.clone())?;
            }
        }
        Ok(())
    }
}

fn snapshot<T: serde::Serialize>(
    context: &mut PollContext<'_>,
    cap: &CapabilityDescriptor,
    topic: &mut TopicState,
    sample: Option<Result<T, Retry>>,
) -> Result<(), ProviderError> {
    match sample {
        Some(Ok(value)) => {
            let payload = serde_json::to_value(value)
                .map_err(|error| ProviderError::Failed(error.to_string()))?;
            context.events.snapshot(cap, &payload)?;
            topic.next_poll = context.now.saturating_add(SAMPLE_INTERVAL);
        }
        Some(Err(retry)) => unavailable(context, cap, topic, retry)?,
        None => {}
    }
    Ok(())
}

fn unavailable(
    context: &mut PollContext<'_>,
    cap: &CapabilityDescriptor,
    topic: &mut TopicState,
    retry: Retry,
) -> Result<(), ProviderError> {
    context
        .health
        .update(cap, CapabilityHealth::Unavailable(retry.reason))?;
    topic.next_poll = retry.at;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_reader::{AccessError, MemoryModule, Target};
    use provider_sdk::{EventSink, HealthSink};

    // Fails immediately if an idle or cached-failure path touches the target.
    struct NoReads;
    impl ProcessMemory for NoReads {
        fn target(&self) -> &Target {
            unreachable!("unexpected target access")
        }
        fn verify(&mut self) -> Result<(), AccessError> {
            unreachable!("unexpected verification")
        }
        fn modules(&mut self) -> Result<Vec<MemoryModule>, AccessError> {
            unreachable!("unexpected module read")
        }
        fn read_into(&mut self, _: u64, _: &mut [u8]) -> Result<(), AccessError> {
            unreachable!("unexpected memory read")
        }
    }

    #[derive(Default)]
    struct Sink {
        resets: Vec<&'static str>,
        health: Vec<(&'static str, CapabilityHealth)>,
        snapshots: Vec<(&'static str, serde_json::Value)>,
    }
    impl EventSink for Sink {
        fn reset(&mut self, cap: &CapabilityDescriptor) -> Result<(), ProviderError> {
            self.resets.push(cap.topic);
            Ok(())
        }
        fn snapshot(
            &mut self,
            cap: &CapabilityDescriptor,
            payload: &serde_json::Value,
        ) -> Result<(), ProviderError> {
            self.snapshots.push((cap.topic, payload.clone()));
            Ok(())
        }
        fn event(
            &mut self,
            _: &CapabilityDescriptor,
            _: &serde_json::Value,
        ) -> Result<(), ProviderError> {
            unreachable!("unexpected event")
        }
    }
    impl HealthSink for Sink {
        fn update(
            &mut self,
            cap: &CapabilityDescriptor,
            health: CapabilityHealth,
        ) -> Result<(), ProviderError> {
            self.health.push((cap.topic, health));
            Ok(())
        }
    }

    #[test]
    fn idle_waiting_and_resumed_demand_do_not_bypass_validation_backoff()
    -> Result<(), ProviderError> {
        let image = Executable {
            base: 0x1_4000_0000,
            actual: crate::target::ExecutableFingerprint {
                timestamp: 0,
                ..crate::target::BUILD
            },
        };
        for failed in 0..4 {
            let mut session = WarframeSession {
                executable: CachedCheck::Passed(image),
                login_layout: CachedCheck::Passed(()),
                string_layout: CachedCheck::Passed(()),
                item_layout: CachedCheck::Passed(()),
                ..WarframeSession::default()
            };
            session.inventory.layout = CachedCheck::Passed(());
            let cap = &INVENTORY;
            let check = match failed {
                0 => &mut session.login_layout,
                1 => &mut session.string_layout,
                2 => &mut session.item_layout,
                _ => &mut session.inventory.layout,
            };
            *check = CachedCheck::Failed(Retry {
                reason: UnavailableReason::UnsupportedBuild,
                at: Duration::from_secs(5),
            });
            let mut events = Sink::default();
            let mut health = Sink::default();
            let demand = [cap];
            let mut context = PollContext {
                demand: &[],
                now: Duration::ZERO,
                memory: &mut NoReads,
                events: &mut events,
                health: &mut health,
            };
            assert_eq!(session.poll(&mut context)?, PollResult::Idle);
            assert!(session.game_build().is_none());
            context.demand = &demand;
            assert_eq!(
                session.poll(&mut context)?,
                PollResult::After(Duration::from_secs(5))
            );
            assert_eq!(
                session.game_build(),
                Some(image.actual.to_string().as_str())
            );
            context.now = Duration::from_secs(1);
            assert_eq!(
                session.poll(&mut context)?,
                PollResult::After(Duration::from_secs(4))
            );
            context.demand = &[];
            assert_eq!(session.poll(&mut context)?, PollResult::Idle);
            context.demand = &demand;
            assert_eq!(
                session.poll(&mut context)?,
                PollResult::After(Duration::from_secs(4))
            );
            assert_eq!(
                health.health,
                vec![
                    (
                        cap.topic,
                        CapabilityHealth::Unavailable(UnavailableReason::UnsupportedBuild)
                    );
                    2
                ]
            );
            assert!(events.resets.is_empty());
            assert!(events.snapshots.is_empty());
        }
        Ok(())
    }

    #[test]
    fn account_reset_wakes_only_demanded_topics_without_revalidating_code()
    -> Result<(), ProviderError> {
        let now = Duration::from_secs(2);
        let mut session = WarframeSession::default();
        for (_, topic) in session.account_topics() {
            topic.layout = CachedCheck::Passed(());
            topic.demand(true, Duration::ZERO);
            topic.next_poll = Duration::from_secs(10);
            assert!(!topic.due(now));
        }
        let mut events = Sink::default();
        session.reset_account(&mut events, now)?;
        for (_, topic) in session.account_topics() {
            assert!(topic.due(now));
            assert!(matches!(topic.layout, CachedCheck::Passed(())));
        }
        session.inventory.demand(false, now);
        session.reset_account(&mut events, now)?;
        assert_eq!(events.resets, [INVENTORY.topic,]);
        assert_eq!(session.schedule(now), PollResult::Idle);
        Ok(())
    }
}
