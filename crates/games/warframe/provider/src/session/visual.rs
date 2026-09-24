//! Shares a single screen source while keeping public topic health independent.

use provider_sdk::{
    CapabilityDescriptor, CapabilityHealth, DependencyFailure, PollContext, ProviderError,
    SnapshotDelivery, UnavailableReason,
    memory::{ReadError, TargetReader},
};
use std::time::Duration;

#[cfg(test)]
#[path = "visual_tests.rs"]
mod tests;
use warframe_model::{RelicRewardPicker, RelicRewardsSnapshot, Screen};

use super::{
    screen_demand::{Consumer, ScreenCondition, ScreenRequirement, ScreenSource, SourcePlan},
    validation::{CachedCheck, Executable, Retry, SharedLayouts},
};
use crate::{
    item_type::ItemTypeCache,
    roots::{self, AccountIdentity, AccountResolution},
    string_pool::StringTokenCache,
    target::READ_LIMITS,
    topics::{
        relic_rewards,
        screens::{self, ScreenSample, UiIdentity},
    },
    world::{self, WorldIdentity},
};

pub(crate) static SCREENS: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.screens",
    schema_version: 1,
    snapshots: Some(SnapshotDelivery::Full),
    events: false,
};
pub(crate) static RELIC_REWARDS: CapabilityDescriptor = CapabilityDescriptor {
    topic: "warframe.relic_rewards",
    schema_version: 1,
    snapshots: Some(SnapshotDelivery::Full),
    events: false,
};
const SCREENS_INTERVAL: Duration = Duration::from_millis(250);
const RELIC_INTERVAL: Duration = Duration::from_millis(50);
const READ_RETRY: Duration = Duration::from_secs(1);

#[derive(Clone, ..Eq)]
struct RelicOwner {
    login: AccountIdentity,
    world: WorldIdentity,
    ui: UiIdentity,
}

struct RelicAcquisition<'a> {
    screens: &'a ScreenSample,
    before: Result<(AccountIdentity, WorldIdentity), Retry>,
    retry_at: Duration,
}

#[derive(Default)]
pub(super) struct VisualTopics {
    source: ScreenSource,
    screens_demanded: bool,
    relic_demanded: bool,
    screen_layout: CachedCheck,
    relic_layout: CachedCheck,
    relic_retry: Duration,
    relic_read_delay: Duration,
    ui: Option<UiIdentity>,
    owner: Option<RelicOwner>,
}

impl VisualTopics {
    pub(super) fn demand(&mut self, context: &PollContext<'_>) {
        let demanded = |cap: &CapabilityDescriptor| {
            context.demand.iter().any(|wanted| {
                wanted.topic == cap.topic && wanted.schema_version == cap.schema_version
            })
        };
        self.screens_demanded = demanded(&SCREENS);
        let relic = demanded(&RELIC_REWARDS);
        if relic && !self.relic_demanded {
            self.relic_retry = context.now;
            self.relic_read_delay = Duration::ZERO;
        }
        self.relic_demanded = relic;
        let mut plan = SourcePlan::default();
        if self.screens_demanded {
            plan.require_screens(
                Consumer::Screens,
                ScreenRequirement {
                    condition: ScreenCondition::Any,
                    preferred_interval: SCREENS_INTERVAL,
                },
            );
        }
        if relic {
            plan.require_screens(
                Consumer::RelicRewards,
                ScreenRequirement {
                    condition: ScreenCondition::Visible(Screen::RelicRewards),
                    preferred_interval: RELIC_INTERVAL,
                },
            );
        }
        self.source.update(plan, context.now);
        if !relic {
            self.owner = None;
        }
        if !relic && !self.screens_demanded {
            self.ui = None;
        }
    }

    pub(super) fn wake(&mut self, cap: &CapabilityDescriptor) {
        if cap.topic == SCREENS.topic || cap.topic == RELIC_REWARDS.topic {
            self.source.wake();
            self.relic_retry = Duration::ZERO;
        }
    }

    pub(super) fn poll_completed(&mut self, committed: bool) {
        if !committed {
            self.source.wake();
            self.relic_retry = Duration::ZERO;
        }
    }

    pub(super) fn deadline(&self) -> Option<Duration> {
        self.source.deadline()
    }
    pub(super) fn due(&self, now: Duration) -> bool {
        self.source.due(now)
    }

    pub(super) fn unavailable(
        &mut self,
        context: &mut PollContext<'_>,
        retry: &Retry,
    ) -> Result<(), ProviderError> {
        for (cap, demanded) in [
            (&SCREENS, self.screens_demanded),
            (&RELIC_REWARDS, self.relic_demanded),
        ] {
            if demanded {
                context
                    .health
                    .update(cap, CapabilityHealth::Unavailable(retry.reason.clone()))?;
            }
        }
        self.source.defer(retry.at);
        Ok(())
    }

    pub(super) fn poll(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
        items: &mut ItemTypeCache,
        strings: &mut StringTokenCache,
        layouts: &mut SharedLayouts,
    ) -> Result<(), ProviderError> {
        let ready = layouts
            .client(context.memory, image, context.now)
            .and_then(|()| {
                self.screen_layout.validate(context.now, SCREENS.topic, || {
                    let mut reader = TargetReader::new(
                        context.memory,
                        image.base,
                        image.actual.image_size,
                        READ_LIMITS,
                    )?;
                    screens::validate(&mut reader)
                })
            });
        if let Err(retry) = ready {
            return self.unavailable(context, &retry);
        }
        // Bracket the shared screen sample with relic ownership reads. A closed
        // picker needs no second screen scan or reward-vector access.
        let retry_at = context
            .now
            .saturating_add(self.relic_read_delay.max(RELIC_INTERVAL));
        let relic_owner = (self.relic_demanded && context.now >= self.relic_retry).then(|| {
            self.validate_relic(context, image, layouts)
                .and_then(|()| Self::resolve_owner(context, image, retry_at))
        });
        let result = TargetReader::new(
            context.memory,
            image.base,
            image.actual.image_size,
            READ_LIMITS,
        )
        .and_then(|mut reader| screens::read(&mut reader))
        .map_err(|error| {
            let delay = if matches!(
                &error,
                ReadError::NotReady { .. } | ReadError::Unstable { .. }
            ) {
                if self.relic_demanded {
                    RELIC_INTERVAL
                } else {
                    SCREENS_INTERVAL
                }
            } else {
                READ_RETRY
            };
            retry(&error, context.now.saturating_add(delay), SCREENS.topic)
        });
        let sample = match result {
            Ok(sample) => sample,
            Err(retry) => return self.unavailable(context, &retry),
        };
        if self.screens_demanded {
            if self.ui.is_some_and(|old| old != sample.owner) {
                context.events.reset(&SCREENS)?;
            }
            publish(context, &SCREENS, &sample.snapshot)?;
        }
        self.ui = Some(sample.owner);
        self.source.sampled(context.now);
        if let Some(before) = relic_owner {
            self.poll_relic(
                context,
                image,
                RelicAcquisition {
                    screens: &sample,
                    before,
                    retry_at,
                },
                items,
                strings,
                layouts,
            )?;
        }
        Ok(())
    }

    fn poll_relic(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
        acquisition: RelicAcquisition<'_>,
        items: &mut ItemTypeCache,
        strings: &mut StringTokenCache,
        layouts: &mut SharedLayouts,
    ) -> Result<(), ProviderError> {
        let mut reset = false;
        let sample = acquisition.screens;
        let retry_at = acquisition.retry_at;
        let result = acquisition.before.and_then(|(login, world)| {
            if world.client != sample.owner.client {
                return Err(retry(
                    &ReadError::changed("reward client"),
                    retry_at,
                    "world ownership",
                ));
            }
            Ok(RelicOwner {
                login,
                world,
                ui: sample.owner,
            })
        });
        let before = match result {
            Ok(owner) => owner,
            Err(retry) => {
                self.clear_owner(context, &mut reset)?;
                return self.relic_unavailable(context, retry);
            }
        };
        if self.owner.as_ref().is_some_and(|old| old != &before) {
            self.clear_owner(context, &mut reset)?;
        }
        self.owner = Some(before.clone());
        let open = self
            .source
            .plan
            .matches(Consumer::RelicRewards, &sample.snapshot);
        let result = if open {
            Self::read_choices(context, image, &before, items, strings, layouts, retry_at)
        } else {
            Ok(RelicRewardPicker::Closed)
        };
        // Check screen visibility as well as login and world ownership around
        // the complete acquisition, including ordering and item resolution.
        let after = Self::recheck_screens(context, image, sample, open, retry_at)
            .and_then(|()| Self::resolve_owner(context, image, retry_at))
            .map(|(login, world)| RelicOwner {
                login,
                world,
                ui: sample.owner,
            });
        match after {
            Ok(after) if before == after => {}
            other => {
                self.clear_owner(context, &mut reset)?;
                let retry = other.err().unwrap_or(Retry {
                    reason: UnavailableReason::TargetNotReady.with_dependency("relic ownership"),
                    at: retry_at,
                });
                return self.relic_unavailable(context, retry);
            }
        }
        match result {
            Ok(picker) => {
                self.relic_read_delay = Duration::ZERO;
                publish(
                    context,
                    &RELIC_REWARDS,
                    &RelicRewardsSnapshot {
                        account_id: before.login.account_id,
                        picker,
                    },
                )
            }
            Err(retry) => self.relic_unavailable(context, retry),
        }
    }

    fn validate_relic(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
        layouts: &mut SharedLayouts,
    ) -> Result<(), Retry> {
        layouts
            .account
            .validate(context.now, "account identity", || {
                roots::validate_account_layout(context.memory, image.base, image.actual.image_size)
            })?;
        layouts.world(context.memory, image, context.now)?;
        self.relic_layout
            .validate(context.now, RELIC_REWARDS.topic, || {
                let mut reader = TargetReader::new(
                    context.memory,
                    image.base,
                    image.actual.image_size,
                    READ_LIMITS,
                )?;
                relic_rewards::validate(&mut reader)
            })
    }

    fn resolve_owner(
        context: &mut PollContext<'_>,
        image: Executable,
        retry_at: Duration,
    ) -> Result<(AccountIdentity, WorldIdentity), Retry> {
        let login =
            match roots::resolve_account(context.memory, image.base, image.actual.image_size) {
                Ok(AccountResolution::Present(login)) => login,
                Ok(AccountResolution::Absent) => {
                    return Err(Retry {
                        reason: UnavailableReason::TargetNotReady
                            .with_dependency("account identity"),
                        at: retry_at,
                    });
                }
                Err(error) => {
                    tracing::debug!(%error, "relic reward login unavailable");
                    return Err(Retry {
                        reason: UnavailableReason::from(&error).with_dependency("account identity"),
                        at: retry_at,
                    });
                }
            };
        let mut reader = TargetReader::new(
            context.memory,
            image.base,
            image.actual.image_size,
            READ_LIMITS,
        )
        .map_err(|error| retry(&error, retry_at, "world ownership"))?;
        let world = world::resolve(&mut reader)
            .map_err(|error| retry(&error, retry_at, "world ownership"))?;
        Ok((login, world))
    }

    fn recheck_screens(
        context: &mut PollContext<'_>,
        image: Executable,
        sample: &ScreenSample,
        open: bool,
        retry_at: Duration,
    ) -> Result<(), Retry> {
        if open {
            let mut reader = TargetReader::new(
                context.memory,
                image.base,
                image.actual.image_size,
                READ_LIMITS,
            )
            .map_err(|error| retry(&error, retry_at, SCREENS.topic))?;
            let after = screens::read(&mut reader)
                .map_err(|error| retry(&error, retry_at, SCREENS.topic))?;
            if after != *sample {
                return Err(retry(
                    &ReadError::changed("reward screen"),
                    retry_at,
                    SCREENS.topic,
                ));
            }
        }
        Ok(())
    }

    fn read_choices(
        context: &mut PollContext<'_>,
        image: Executable,
        owner: &RelicOwner,
        items: &mut ItemTypeCache,
        strings: &mut StringTokenCache,
        layouts: &mut SharedLayouts,
        retry_at: Duration,
    ) -> Result<RelicRewardPicker, Retry> {
        layouts.item_paths(context.memory, image, context.now)?;
        let rules = owner.world.rules.ok_or_else(|| {
            retry(
                &ReadError::changed("reward world"),
                retry_at,
                "world ownership",
            )
        })?;
        let mut reader = TargetReader::new(
            context.memory,
            image.base,
            image.actual.image_size,
            READ_LIMITS,
        )
        .map_err(|error| retry(&error, retry_at, RELIC_REWARDS.topic))?;
        relic_rewards::read(&mut reader, rules, &owner.login.account_id, items, strings)
            .map(|choices| RelicRewardPicker::Open { choices })
            .map_err(|error| retry(&error, retry_at, RELIC_REWARDS.topic))
    }

    fn clear_owner(
        &mut self,
        context: &mut PollContext<'_>,
        reset: &mut bool,
    ) -> Result<(), ProviderError> {
        if self.owner.take().is_some() && !*reset {
            context.events.reset(&RELIC_REWARDS)?;
            *reset = true;
        }
        Ok(())
    }

    fn relic_unavailable(
        &mut self,
        context: &mut PollContext<'_>,
        mut retry: Retry,
    ) -> Result<(), ProviderError> {
        if retry.reason.failure() == DependencyFailure::TargetNotReady {
            // Missing reward items and changing owners are expected during
            // transitions; retain the requested cadence until they settle.
            retry.at = context.now.saturating_add(RELIC_INTERVAL);
            self.relic_read_delay = Duration::ZERO;
        } else {
            // Back off persistent malformed/unreadable data, retaining the
            // independent longer deadlines for cached layout failures.
            self.relic_read_delay = self
                .relic_read_delay
                .max(RELIC_INTERVAL)
                .saturating_mul(2)
                .min(READ_RETRY);
        }
        tracing::debug!(
            reason = ?retry.reason,
            retry_ms = retry.at.saturating_sub(context.now).as_millis(),
            "relic reward acquisition deferred"
        );
        context
            .health
            .update(&RELIC_REWARDS, CapabilityHealth::Unavailable(retry.reason))?;
        self.relic_retry = retry.at;
        Ok(())
    }
}

fn retry(error: &ReadError, at: Duration, dependency: &'static str) -> Retry {
    tracing::debug!(%error, "visual topic acquisition unavailable");
    Retry {
        reason: UnavailableReason::from(error).with_dependency(dependency),
        at,
    }
}

fn publish(
    context: &mut PollContext<'_>,
    cap: &CapabilityDescriptor,
    value: &impl serde::Serialize,
) -> Result<(), ProviderError> {
    let value =
        serde_json::to_value(value).map_err(|error| ProviderError::Failed(error.to_string()))?;
    context.events.snapshot(cap, &value)
}
