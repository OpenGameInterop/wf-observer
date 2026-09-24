use super::validation::{CachedCheck, Executable, Retry, SharedLayouts, identify};
use crate::{
    item_type::ItemTypeCache,
    roots::{self, AccountIdentity, AccountResolution, ProfileDataIdentity},
    string_pool::StringTokenCache,
    topics,
};
use provider_sdk::{
    CapabilityDescriptor, CapabilityHealth, PollContext, PollResult, ProviderError,
    ProviderSession, SnapshotDelivery, UnavailableReason,
};
use std::time::Duration;
use warframe_model::{ChatEvent, InventorySnapshot, MasterySnapshot, PlayerSnapshot};

#[path = "profile_data.rs"]
mod profile_data;

#[cfg(test)]
#[path = "acquisition_tests.rs"]
mod acquisition_tests;

#[cfg(test)]
#[path = "mastery_tests.rs"]
mod mastery_tests;

#[cfg(test)]
#[path = "progression_tests.rs"]
mod progression_tests;

#[cfg(test)]
#[path = "chat_tests.rs"]
mod chat_tests;

#[cfg(test)]
#[path = "dependency_tests.rs"]
mod dependency_tests;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const CHAT_INTERVAL: Duration = Duration::from_millis(250);
const CHAT_BACKLOG_INTERVAL: Duration = Duration::from_millis(25);

pub(crate) static MASTERY: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.mastery",
    schema_version: 1,
    snapshots: Some(SnapshotDelivery::Delta),
    events: false,
};

pub(crate) static INTRINSICS: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.intrinsics",
    schema_version: 1,
    snapshots: Some(SnapshotDelivery::Full),
    events: false,
};

pub(crate) static STAR_CHART: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.star_chart",
    schema_version: 1,
    snapshots: Some(SnapshotDelivery::Delta),
    events: false,
};

pub(crate) static INVENTORY: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.inventory",
    schema_version: 1,
    snapshots: Some(SnapshotDelivery::Delta),
    events: false,
};

pub(crate) static CURRENCIES: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.currencies",
    schema_version: 1,
    snapshots: Some(SnapshotDelivery::Full),
    events: false,
};

pub(crate) static PLAYER: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.player",
    schema_version: 1,
    snapshots: Some(SnapshotDelivery::Full),
    events: false,
};

pub(crate) static CHAT: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.chat",
    schema_version: 1,
    snapshots: None,
    events: true,
};

#[derive(Default)]
pub(crate) struct WarframeSession {
    visual: super::visual::VisualTopics,
    executable: CachedCheck<Executable>,
    build_name: Option<String>,
    layouts: SharedLayouts,
    inventory: TopicState,
    mastery: TopicState,
    intrinsics: TopicState,
    star_chart: TopicState,
    currencies: TopicState,
    player: TopicState,
    chat: TopicState,
    login: Option<AccountIdentity>,
    profile_data: Option<ProfileDataIdentity>,
    items: ItemTypeCache,
    strings: StringTokenCache,
    chat_history: Option<topics::ChatHistory>,
    chat_cursor: topics::ChatCursor,
    pending_chat: Option<PendingChat>,
    chat_staged: bool,
}

/// One bounded publication, retained across rejection even if native entries expire.
struct PendingChat {
    history: Option<topics::ChatHistory>,
    cursor: topics::ChatCursor,
    events: Vec<ChatEvent>,
    more: bool,
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
        self.visual.wake(cap);
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
        self.visual.poll_completed(committed);
        // An unrelated accepted poll must not commit a pending chat retry.
        if !std::mem::take(&mut self.chat_staged) {
            return;
        }
        if committed {
            if let Some(pending) = self.pending_chat.take() {
                if let Some(history) = pending.history {
                    self.chat_history = Some(history);
                }
                self.chat_cursor = pending.cursor;
            }
        } else {
            self.chat.next_poll = Duration::ZERO;
        }
    }

    fn game_build(&self) -> Option<&str> {
        self.build_name.as_deref()
    }

    fn poll(&mut self, context: &mut PollContext<'_>) -> Result<PollResult, ProviderError> {
        self.visual.demand(context);
        if self.visual.due(context.now) {
            match self
                .executable
                .ensure(context.now, || identify(context.memory))
            {
                Ok(image) => {
                    self.build_name
                        .get_or_insert_with(|| image.actual.to_string());
                    self.visual.poll(
                        context,
                        image,
                        &mut self.items,
                        &mut self.strings,
                        &mut self.layouts,
                    )?;
                }
                Err(retry) => self.visual.unavailable(context, &retry)?,
            }
        }
        for (cap, topic) in self.account_topics() {
            let demanded = context.demand.iter().any(|wanted| {
                wanted.topic == cap.topic && wanted.schema_version == cap.schema_version
            });
            topic.demand(demanded, context.now);
        }
        if !self.chat.demanded {
            self.clear_chat();
        }
        if !self.data_topics().iter().any(|(_, topic)| topic.demanded) {
            self.profile_data = None;
        }
        if !self
            .account_topics()
            .iter()
            .any(|(_, topic)| topic.demanded)
        {
            self.login = None;
            return Ok(self.schedule(context.now));
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
                    self.layouts
                        .account
                        .validate(context.now, "account identity", || {
                            roots::validate_account_layout(
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
    fn account_topics(&mut self) -> [(&'static CapabilityDescriptor, &mut TopicState); 7] {
        [
            (&INVENTORY, &mut self.inventory),
            (&CURRENCIES, &mut self.currencies),
            (&PLAYER, &mut self.player),
            (&CHAT, &mut self.chat),
            (&MASTERY, &mut self.mastery),
            (&INTRINSICS, &mut self.intrinsics),
            (&STAR_CHART, &mut self.star_chart),
        ]
    }

    fn schedule(&mut self, now: Duration) -> PollResult {
        let visual = self.visual.deadline().map(|at| at.saturating_sub(now));
        self.account_topics()
            .into_iter()
            .filter(|(_, topic)| topic.demanded)
            .map(|(_, topic)| topic.next_poll.saturating_sub(now))
            .chain(visual)
            .min()
            .map_or(PollResult::Idle, PollResult::After)
    }

    fn acquire(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
    ) -> Result<(), ProviderError> {
        let Some((before, reset)) = self.account_before(context, image)? else {
            return Ok(());
        };
        let mut data_reset = reset;
        let data = self.profile_data_before(context, image, &before, &mut data_reset)?;
        let inventory = data
            .as_ref()
            .filter(|_| self.inventory.due(context.now))
            .map(|owner| self.sample_inventory(context, image, owner));
        let currencies = data
            .as_ref()
            .filter(|_| self.currencies.due(context.now))
            .map(|owner| {
                topics::read_currencies(context.memory, image.base, image.actual.image_size, owner)
                    .map_err(|error| read_retry(&error, context.now, CURRENCIES.topic))
            });
        let mastery = data
            .as_ref()
            .filter(|_| self.mastery.due(context.now))
            .map(|owner| self.sample_mastery(context, image, owner));
        let intrinsics = data
            .as_ref()
            .filter(|_| self.intrinsics.due(context.now))
            .map(|owner| {
                topics::read_intrinsics(context.memory, image.base, image.actual.image_size, owner)
                    .map_err(|error| read_retry(&error, context.now, INTRINSICS.topic))
            });
        let star_chart = data
            .as_ref()
            .filter(|_| self.star_chart.due(context.now))
            .map(|owner| {
                topics::read_star_chart(
                    context.memory,
                    image.base,
                    image.actual.image_size,
                    owner,
                    &mut self.strings,
                )
                .map_err(|error| read_retry(&error, context.now, STAR_CHART.topic))
            });
        let player_due = self.player.due(context.now);
        let name_needed = self.chat.due(context.now) && self.pending_chat.is_none();
        // Optional chat enrichment shares the coherent login sample, never a cached username.
        // A failure here affects player health only when player itself is due.
        let player =
            (player_due || name_needed).then(|| self.sample_player(context, image, &before));
        let local_name = player
            .as_ref()
            .and_then(|sample| sample.as_ref().ok())
            .map(|sample| sample.username.as_str().to_owned());
        let player = if player_due { player } else { None };
        let chat = data
            .as_ref()
            .filter(|_| self.chat.due(context.now))
            .map(|owner| {
                if self.pending_chat.is_some() {
                    return Ok(None);
                }
                topics::read_chat(
                    context.memory,
                    image.base,
                    image.actual.image_size,
                    owner,
                    self.chat_history.as_ref(),
                )
                .map_err(|error| read_retry(&error, context.now, CHAT.topic))
            });
        let data_after = data.as_ref().map(|_| {
            roots::resolve_profile_data(
                context.memory,
                image.base,
                image.actual.image_size,
                &before,
            )
        });
        // Even a failed acquisition must not retain a previous account's data.
        // Results stay local until all account-scoped reads have completed.
        if !self.account_unchanged(context, image, &before, reset, data_reset)? {
            return Ok(());
        }
        snapshot(context, &PLAYER, &mut self.player, player)?;
        if let Some(owner) = &data
            && !matches!(&data_after, Some(Ok(Some(after))) if after == owner)
        {
            self.clear_profile_data(context, &mut data_reset)?;
            return self.data_unavailable(
                context,
                &Retry {
                    reason: UnavailableReason::TargetNotReady,
                    at: context.now.saturating_add(SAMPLE_INTERVAL),
                },
            );
        }
        snapshot(context, &INVENTORY, &mut self.inventory, inventory)?;
        snapshot(context, &CURRENCIES, &mut self.currencies, currencies)?;
        snapshot(context, &MASTERY, &mut self.mastery, mastery)?;
        snapshot(context, &INTRINSICS, &mut self.intrinsics, intrinsics)?;
        snapshot(context, &STAR_CHART, &mut self.star_chart, star_chart)?;
        self.publish_chat(context, &before, chat, local_name.as_deref())
    }

    fn account_before(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
    ) -> Result<Option<(AccountIdentity, bool)>, ProviderError> {
        let before =
            match roots::resolve_account(context.memory, image.base, image.actual.image_size) {
                Ok(AccountResolution::Present(login)) => login,
                Ok(AccountResolution::Absent) => {
                    self.clear_login(context)?;
                    self.account_unavailable(
                        context,
                        &Retry {
                            reason: UnavailableReason::TargetNotReady,
                            at: context.now.saturating_add(SAMPLE_INTERVAL),
                        },
                    )?;
                    return Ok(None);
                }
                Err(error) => {
                    tracing::debug!(%error, "account roots unavailable");
                    self.clear_login(context)?;
                    self.account_unavailable(
                        context,
                        &Retry {
                            reason: (&error).into(),
                            at: context.now.saturating_add(SAMPLE_INTERVAL),
                        },
                    )?;
                    return Ok(None);
                }
            };
        let reset = self.login.as_ref().is_some_and(|old| old != &before);
        if reset {
            self.reset_account(context.events, context.now)?;
            // Reset also wakes previously deferred topics; their cached failures still apply.
            self.validate_due(context, image)?;
        }
        self.login = Some(before.clone());
        Ok(Some((before, reset)))
    }

    fn account_unchanged(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
        before: &AccountIdentity,
        reset: bool,
        data_reset: bool,
    ) -> Result<bool, ProviderError> {
        let after = roots::resolve_account(context.memory, image.base, image.actual.image_size);
        if !matches!(&after, Ok(AccountResolution::Present(login)) if login == before) {
            if !reset {
                if data_reset {
                    self.reset_player(context.events, context.now)?;
                } else {
                    self.reset_account(context.events, context.now)?;
                }
            }
            self.login = None;
            self.profile_data = None;
            self.clear_chat();
            self.account_unavailable(
                context,
                &Retry {
                    reason: UnavailableReason::TargetNotReady,
                    at: context.now.saturating_add(SAMPLE_INTERVAL),
                },
            )?;
            return Ok(false);
        }
        Ok(true)
    }

    fn publish_chat(
        &mut self,
        context: &mut PollContext<'_>,
        before: &AccountIdentity,
        chat: Option<Result<Option<topics::ChatHistory>, Retry>>,
        local_name: Option<&str>,
    ) -> Result<(), ProviderError> {
        match chat {
            Some(Ok(history)) => {
                if self.pending_chat.is_none() {
                    let current = history
                        .as_ref()
                        .or(self.chat_history.as_ref())
                        .ok_or_else(|| ProviderError::Failed("chat baseline missing".into()))?;
                    let (cursor, updates, more) = self
                        .chat_cursor
                        .prepare(current, local_name)
                        .map_err(|error| ProviderError::Failed(error.to_string()))?;
                    self.pending_chat = Some(PendingChat {
                        history,
                        cursor,
                        events: updates
                            .into_iter()
                            .map(|update| ChatEvent {
                                account_id: before.account_id.clone(),
                                update,
                            })
                            .collect(),
                        more,
                    });
                }
                let pending = self
                    .pending_chat
                    .as_ref()
                    .ok_or_else(|| ProviderError::Failed("chat publication missing".into()))?;
                // Retain both events and tentative positions before any sink can fail.
                self.chat_staged = true;
                context.health.update(&CHAT, CapabilityHealth::Available)?;
                for event in &pending.events {
                    let payload = serde_json::to_value(event)
                        .map_err(|error| ProviderError::Failed(error.to_string()))?;
                    context.events.event(&CHAT, &payload)?;
                }
                self.chat.next_poll = context.now.saturating_add(if pending.more {
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

    fn sample_player(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
        login: &AccountIdentity,
    ) -> Result<PlayerSnapshot, Retry> {
        self.player.layout.validate(context.now, PLAYER.topic, || {
            topics::validate_player_layout(context.memory, image.base, image.actual.image_size)
        })?;
        topics::read_player(context.memory, image.base, image.actual.image_size, login)
            .map_err(|error| read_retry(&error, context.now, PLAYER.topic))
    }

    fn sample_inventory(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
        login: &ProfileDataIdentity,
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

    fn sample_mastery(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
        login: &ProfileDataIdentity,
    ) -> Result<MasterySnapshot, Retry> {
        topics::read_mastery(
            context.memory,
            image.base,
            image.actual.image_size,
            login,
            &mut self.items,
            &mut self.strings,
        )
        .map_err(|error| {
            tracing::debug!(%error, "mastery acquisition unavailable");
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
        self.validate_profile_data(context, image)?;
        if self.inventory.due(context.now) {
            let ready = self
                .layouts
                .item_paths(context.memory, image, context.now)
                .and_then(|()| {
                    self.layouts
                        .inventory_owner(context.memory, image, context.now)
                })
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
        self.validate_progression_due(context, image)?;
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

    fn validate_progression_due(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
    ) -> Result<(), ProviderError> {
        if self.mastery.due(context.now) {
            let ready = self
                .layouts
                .item_paths(context.memory, image, context.now)
                .and_then(|()| {
                    self.layouts
                        .inventory_owner(context.memory, image, context.now)
                })
                .and_then(|()| {
                    self.mastery
                        .layout
                        .validate(context.now, MASTERY.topic, || {
                            topics::validate_mastery_layout(
                                context.memory,
                                image.base,
                                image.actual.image_size,
                            )
                        })
                });
            if let Err(retry) = ready {
                unavailable(context, &MASTERY, &mut self.mastery, retry)?;
            }
        }
        if self.intrinsics.due(context.now) {
            let ready = self
                .layouts
                .profile_commit(context.memory, image, context.now)
                .and_then(|()| {
                    self.intrinsics
                        .layout
                        .validate(context.now, INTRINSICS.topic, || {
                            topics::validate_intrinsics_layout(
                                context.memory,
                                image.base,
                                image.actual.image_size,
                            )
                        })
                });
            if let Err(retry) = ready {
                unavailable(context, &INTRINSICS, &mut self.intrinsics, retry)?;
            }
        }
        if self.star_chart.due(context.now) {
            let ready = self
                .layouts
                .profile_commit(context.memory, image, context.now)
                .and_then(|()| {
                    self.layouts
                        .string_tokens(context.memory, image, context.now)
                })
                .and_then(|()| {
                    self.star_chart
                        .layout
                        .validate(context.now, STAR_CHART.topic, || {
                            topics::validate_star_chart_layout(
                                context.memory,
                                image.base,
                                image.actual.image_size,
                            )
                        })
                });
            if let Err(retry) = ready {
                unavailable(context, &STAR_CHART, &mut self.star_chart, retry)?;
            }
        }
        Ok(())
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
        // Preserve the established publication order while resetting every
        // account-owned topic. A data-only replacement uses the smaller group.
        self.profile_data = None;
        self.clear_chat();
        for (cap, topic) in self.account_topics() {
            if topic.demanded {
                events.reset(cap)?;
                topic.next_poll = now;
            }
        }
        Ok(())
    }

    fn reset_player(
        &mut self,
        events: &mut dyn provider_sdk::EventSink,
        now: Duration,
    ) -> Result<(), ProviderError> {
        if self.player.demanded {
            events.reset(&PLAYER)?;
            self.player.next_poll = now;
        }
        Ok(())
    }

    fn clear_chat(&mut self) {
        self.chat_history = None;
        self.chat_cursor = topics::ChatCursor::default();
        self.pending_chat = None;
        self.chat_staged = false;
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
    use memory_reader::{AccessError, MemoryModule, ProcessMemory, Target};
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
                layouts: SharedLayouts::validated(),
                ..WarframeSession::default()
            };
            session.inventory.layout = CachedCheck::Passed(());
            let cap = if failed >= 4 {
                session.layouts.strings = CachedCheck::Unchecked;
                session.layouts.items = CachedCheck::Unchecked;
                match failed {
                    4 => &CURRENCIES,
                    5 => &PLAYER,
                    _ => &CHAT,
                }
            } else {
                &INVENTORY
            };
            let check = match failed {
                0 => &mut session.layouts.account,
                1 => &mut session.layouts.strings,
                2 => &mut session.layouts.items,
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
                MASTERY.topic,
                INTRINSICS.topic,
                STAR_CHART.topic,
                CURRENCIES.topic,
                PLAYER.topic,
                CHAT.topic,
                MASTERY.topic,
                INTRINSICS.topic,
                STAR_CHART.topic
            ]
        );
        session.currencies.demand(false, now);
        session.player.demand(false, now);
        session.chat.demand(false, now);
        session.mastery.demand(false, now);
        session.intrinsics.demand(false, now);
        session.star_chart.demand(false, now);
        assert_eq!(session.schedule(now), PollResult::Idle);
        Ok(())
    }

    #[test]
    fn topic_validation_and_acquisition_failures_stay_isolated()
    -> Result<(), Box<dyn std::error::Error>> {
        use warframe_model::{AccountId, CurrencyBalances, CurrencySnapshot};
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
                layouts: SharedLayouts::validated(),
                ..WarframeSession::default()
            };
            for (_, topic) in session.account_topics() {
                topic.demand(true, Duration::ZERO);
                topic.layout = CachedCheck::Passed(());
            }
            let check = match blocked {
                0 => &mut session.layouts.strings,
                1 => &mut session.layouts.items,
                2 => &mut session.inventory.layout,
                3 => &mut session.currencies.layout,
                4 => &mut session.player.layout,
                _ => &mut session.chat.layout,
            };
            *check = CachedCheck::Failed(failed.clone());
            let mut events = Sink::default();
            let mut health = Sink::default();
            let mut context = PollContext {
                demand: &[&INVENTORY, &CURRENCIES, &PLAYER, &CHAT, &MASTERY],
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
            snapshot(
                &mut context,
                &INVENTORY,
                &mut session.inventory,
                Some(if blocked == 3 {
                    Ok(inventory)
                } else {
                    Err(failed.clone())
                }),
            )?;
            snapshot(
                &mut context,
                &CURRENCIES,
                &mut session.currencies,
                Some(if blocked == 3 {
                    Err(failed.clone())
                } else {
                    Ok(currencies)
                }),
            )?;
            let (healthy, broken) = if blocked == 3 {
                (&INVENTORY, &CURRENCIES)
            } else {
                (&CURRENCIES, &INVENTORY)
            };
            assert_eq!(events.snapshots.len(), 1);
            assert_eq!(events.snapshots[0].0, healthy.topic);
            assert!(health.health.iter().all(|(topic, _)| *topic == broken.topic
                || (blocked <= 1 && *topic == MASTERY.topic)
                || (blocked == 0 && *topic == STAR_CHART.topic)));
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
