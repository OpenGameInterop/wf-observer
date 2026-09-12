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
use warframe_model::{ChatEvent, CurrencySnapshot, InventorySnapshot, PlayerSnapshot};

#[cfg(test)]
#[path = "acquisition_tests.rs"]
mod acquisition_tests;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const CHAT_INTERVAL: Duration = Duration::from_millis(250);
const CHAT_BACKLOG_INTERVAL: Duration = Duration::from_millis(25);

pub(crate) static INVENTORY: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.inventory",
    schema_version: 1,
    snapshots: true,
    events: false,
};

pub(crate) static CURRENCIES: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.currencies",
    schema_version: 1,
    snapshots: true,
    events: false,
};

pub(crate) static PLAYER: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.player",
    schema_version: 1,
    snapshots: true,
    events: false,
};

pub(crate) static CHAT: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.chat",
    schema_version: 1,
    snapshots: false,
    events: true,
};

#[derive(Default)]
pub(crate) struct WarframeSession {
    executable: CachedCheck<Executable>,
    build_name: Option<String>,
    login_layout: CachedCheck,
    string_layout: CachedCheck,
    item_layout: CachedCheck,
    inventory: TopicState,
    currencies: TopicState,
    player: TopicState,
    chat: TopicState,
    login: Option<LoginIdentity>,
    items: ItemTypeCache,
    strings: StringTokenCache,
    chat_history: Option<topics::ChatHistory>,
    chat_cursor: topics::ChatCursor,
    pending_chat: Option<(Option<topics::ChatHistory>, topics::ChatCursor)>,
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
        if cap.topic == CHAT.topic {
            self.clear_chat();
        }
        for (own, topic) in self.account_topics() {
            if own.topic == cap.topic {
                topic.next_poll = Duration::ZERO;
            }
        }
    }

    fn poll_completed(&mut self, committed: bool) {
        if let Some((history, cursor)) = self.pending_chat.take() {
            if committed {
                if let Some(history) = history {
                    self.chat_history = Some(history);
                }
                self.chat_cursor = cursor;
            } else {
                self.chat.next_poll = Duration::ZERO;
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
        if !self.chat.demanded {
            self.clear_chat();
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
    fn account_topics(&mut self) -> [(&'static CapabilityDescriptor, &mut TopicState); 4] {
        [
            (&INVENTORY, &mut self.inventory),
            (&CURRENCIES, &mut self.currencies),
            (&PLAYER, &mut self.player),
            (&CHAT, &mut self.chat),
        ]
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
        let currencies = self.currencies.due(context.now).then(|| {
            topics::read_currencies(context.memory, image.base, image.actual.image_size, &before)
                .map_err(|error| read_retry(&error, context.now, CURRENCIES.topic))
        });
        let player = self.player.due(context.now).then(|| {
            topics::read_player(context.memory, image.base, image.actual.image_size, &before)
                .map_err(|error| read_retry(&error, context.now, PLAYER.topic))
        });
        let chat = self.chat.due(context.now).then(|| {
            topics::read_chat(
                context.memory,
                image.base,
                image.actual.image_size,
                &before,
                self.chat_history.as_ref(),
            )
            .map_err(|error| read_retry(&error, context.now, CHAT.topic))
        });
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
        self.publish(context, inventory, currencies, player)?;
        match chat {
            Some(Ok(history)) => {
                let current = history
                    .as_ref()
                    .or(self.chat_history.as_ref())
                    .ok_or_else(|| ProviderError::Failed("chat baseline missing".into()))?;
                let (cursor, updates, more) = self.chat_cursor.prepare(current);
                // Stage the next position before any sink operation can fail.
                self.pending_chat = Some((history, cursor));
                context.health.update(&CHAT, CapabilityHealth::Available)?;
                for update in updates {
                    let payload = serde_json::to_value(ChatEvent {
                        account_id: before.account_id.clone(),
                        update,
                    })
                    .map_err(|error| ProviderError::Failed(error.to_string()))?;
                    context.events.event(&CHAT, &payload)?;
                }
                self.chat.next_poll = context.now.saturating_add(if more {
                    CHAT_BACKLOG_INTERVAL
                } else {
                    CHAT_INTERVAL
                });
            }
            Some(Err(retry)) => unavailable(context, &CHAT, &mut self.chat, retry)?,
            None => {}
        }
        Ok(())
    }

    /// Called only after the shared ownership recheck; an acquisition error affects only its topic.
    fn publish(
        &mut self,
        context: &mut PollContext<'_>,
        inventory: Option<Result<InventorySnapshot, Retry>>,
        currencies: Option<Result<CurrencySnapshot, Retry>>,
        player: Option<Result<PlayerSnapshot, Retry>>,
    ) -> Result<(), ProviderError> {
        snapshot(context, &INVENTORY, &mut self.inventory, inventory)?;
        snapshot(context, &CURRENCIES, &mut self.currencies, currencies)?;
        snapshot(context, &PLAYER, &mut self.player, player)
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
        if self.currencies.due(context.now) {
            let ready = self
                .currencies
                .layout
                .validate(context.now, CURRENCIES.topic, || {
                    topics::validate_currencies_layout(
                        context.memory,
                        image.base,
                        image.actual.image_size,
                    )
                });
            if let Err(retry) = ready {
                unavailable(context, &CURRENCIES, &mut self.currencies, retry)?;
            }
        }
        if self.player.due(context.now) {
            let ready = self.player.layout.validate(context.now, PLAYER.topic, || {
                topics::validate_player_layout(context.memory, image.base, image.actual.image_size)
            });
            if let Err(retry) = ready {
                unavailable(context, &PLAYER, &mut self.player, retry)?;
            }
        }
        if self.chat.due(context.now) {
            let ready = self.chat.layout.validate(context.now, CHAT.topic, || {
                topics::validate_chat_layout(context.memory, image.base, image.actual.image_size)
            });
            if let Err(retry) = ready {
                unavailable(context, &CHAT, &mut self.chat, retry)?;
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
        self.clear_chat();
        for (cap, topic) in self.account_topics() {
            if topic.demanded {
                events.reset(cap)?;
                topic.next_poll = now;
            }
        }
        Ok(())
    }

    fn clear_chat(&mut self) {
        self.chat_history = None;
        self.chat_cursor = topics::ChatCursor::default();
        self.pending_chat = None;
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

fn read_retry(
    error: &provider_sdk::memory::ReadError,
    now: Duration,
    topic: &'static str,
) -> Retry {
    tracing::debug!(%error, topic, "topic acquisition unavailable");
    Retry {
        reason: error.into(),
        at: now.saturating_add(SAMPLE_INTERVAL),
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
        for failed in 0..7 {
            let mut session = WarframeSession {
                executable: CachedCheck::Passed(image),
                login_layout: CachedCheck::Passed(()),
                string_layout: CachedCheck::Passed(()),
                item_layout: CachedCheck::Passed(()),
                ..WarframeSession::default()
            };
            session.inventory.layout = CachedCheck::Passed(());
            let cap = if failed >= 4 {
                session.string_layout = CachedCheck::Unchecked;
                session.item_layout = CachedCheck::Unchecked;
                match failed {
                    4 => &CURRENCIES,
                    5 => &PLAYER,
                    _ => &CHAT,
                }
            } else {
                &INVENTORY
            };
            let check = match failed {
                0 => &mut session.login_layout,
                1 => &mut session.string_layout,
                2 => &mut session.item_layout,
                4 => &mut session.currencies.layout,
                5 => &mut session.player.layout,
                6 => &mut session.chat.layout,
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
        assert_eq!(
            events.resets,
            [
                INVENTORY.topic,
                CURRENCIES.topic,
                PLAYER.topic,
                CHAT.topic,
                CURRENCIES.topic,
                PLAYER.topic,
                CHAT.topic
            ]
        );
        session.currencies.demand(false, now);
        session.player.demand(false, now);
        session.chat.demand(false, now);
        assert_eq!(session.schedule(now), PollResult::Idle);
        Ok(())
    }

    #[test]
    fn topic_validation_and_acquisition_failures_stay_isolated()
    -> Result<(), Box<dyn std::error::Error>> {
        use warframe_model::{AccountId, CurrencyBalances};
        let image = Executable {
            base: 0x1_4000_0000,
            actual: crate::target::BUILD,
        };
        let failed = Retry {
            reason: UnavailableReason::UnsupportedBuild,
            at: Duration::from_secs(5),
        };
        for blocked in 0..6 {
            let mut session = WarframeSession {
                string_layout: CachedCheck::Passed(()),
                item_layout: CachedCheck::Passed(()),
                ..WarframeSession::default()
            };
            for (_, topic) in session.account_topics() {
                topic.demand(true, Duration::ZERO);
                topic.layout = CachedCheck::Passed(());
            }
            let check = match blocked {
                0 => &mut session.string_layout,
                1 => &mut session.item_layout,
                2 => &mut session.inventory.layout,
                3 => &mut session.currencies.layout,
                4 => &mut session.player.layout,
                _ => &mut session.chat.layout,
            };
            *check = CachedCheck::Failed(failed.clone());
            let mut events = Sink::default();
            let mut health = Sink::default();
            let mut context = PollContext {
                demand: &[&INVENTORY, &CURRENCIES, &PLAYER, &CHAT],
                now: Duration::ZERO,
                memory: &mut NoReads,
                events: &mut events,
                health: &mut health,
            };
            session.validate_due(&mut context, image)?;
            assert_eq!(session.inventory.due(context.now), blocked >= 3);
            assert_eq!(session.currencies.due(context.now), blocked != 3);
            assert_eq!(session.player.due(context.now), blocked != 4);
            assert_eq!(session.chat.due(context.now), blocked != 5);
            // Wake/recheck must retain the failed topic's own backoff.
            context.now = Duration::from_secs(1);
            session.reset_account(context.events, context.now)?;
            session.validate_due(&mut context, image)?;
            assert_eq!(session.inventory.due(context.now), blocked >= 3);
            assert_eq!(session.currencies.due(context.now), blocked != 3);
            assert_eq!(session.player.due(context.now), blocked != 4);
            assert_eq!(session.chat.due(context.now), blocked != 5);
            if blocked >= 4 {
                let broken = if blocked == 4 {
                    PLAYER.topic
                } else {
                    CHAT.topic
                };
                assert!(health.health.iter().all(|(topic, _)| *topic == broken));
                continue;
            }

            let account_id = AccountId::new("0123456789abcdef01234567")?;
            let inventory = empty_inventory(account_id.clone())?;
            let currencies = CurrencySnapshot {
                account_id,
                balances: CurrencyBalances {
                    credits: 1,
                    endo: 2,
                    tradable_platinum: 3,
                    non_tradable_platinum: 0,
                },
            };
            session.publish(
                &mut context,
                Some(if blocked == 3 {
                    Ok(inventory)
                } else {
                    Err(failed.clone())
                }),
                Some(if blocked == 3 {
                    Err(failed.clone())
                } else {
                    Ok(currencies)
                }),
                None,
            )?;
            let (healthy, broken) = if blocked == 3 {
                (&INVENTORY, &CURRENCIES)
            } else {
                (&CURRENCIES, &INVENTORY)
            };
            assert_eq!(events.snapshots.len(), 1);
            assert_eq!(events.snapshots[0].0, healthy.topic);
            assert!(
                health
                    .health
                    .iter()
                    .all(|(topic, _)| *topic == broken.topic)
            );
        }
        Ok(())
    }

    fn empty_inventory(
        account_id: warframe_model::AccountId,
    ) -> Result<InventorySnapshot, warframe_model::InvalidInventory> {
        use warframe_model::{InventoryFamily, InventoryFamilySnapshot};
        InventorySnapshot::new(
            account_id,
            InventoryFamily::ALL
                .iter()
                .map(|&family| InventoryFamilySnapshot {
                    family,
                    items: vec![],
                })
                .collect(),
        )
    }
}
