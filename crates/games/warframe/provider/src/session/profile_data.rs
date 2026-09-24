//! Profile-data validation and lifetime affect only its six consumers.
use super::{
    AccountIdentity, CHAT, CURRENCIES, CapabilityDescriptor, Executable, INTRINSICS, INVENTORY,
    MASTERY, PollContext, ProfileDataIdentity, ProviderError, Retry, SAMPLE_INTERVAL, STAR_CHART,
    TopicState, UnavailableReason, WarframeSession, roots, unavailable,
};

impl WarframeSession {
    pub(super) fn data_topics(&mut self) -> [(&'static CapabilityDescriptor, &mut TopicState); 6] {
        [
            (&INVENTORY, &mut self.inventory),
            (&CURRENCIES, &mut self.currencies),
            (&CHAT, &mut self.chat),
            (&MASTERY, &mut self.mastery),
            (&INTRINSICS, &mut self.intrinsics),
            (&STAR_CHART, &mut self.star_chart),
        ]
    }

    pub(super) fn validate_profile_data(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
    ) -> Result<(), ProviderError> {
        if self
            .data_topics()
            .iter()
            .any(|(_, topic)| topic.due(context.now))
        {
            let ready = self
                .layouts
                .profile_data
                .validate(context.now, "profile data", || {
                    roots::validate_profile_data_layout(
                        context.memory,
                        image.base,
                        image.actual.image_size,
                    )
                });
            if let Err(retry) = ready {
                self.data_unavailable(context, &retry)?;
            }
        }
        Ok(())
    }

    pub(super) fn profile_data_before(
        &mut self,
        context: &mut PollContext<'_>,
        image: Executable,
        account: &AccountIdentity,
        reset: &mut bool,
    ) -> Result<Option<ProfileDataIdentity>, ProviderError> {
        if !self
            .data_topics()
            .iter()
            .any(|(_, topic)| topic.due(context.now))
        {
            return Ok(None);
        }
        let result = roots::resolve_profile_data(
            context.memory,
            image.base,
            image.actual.image_size,
            account,
        );
        let owner = match result {
            Ok(Some(owner)) => owner,
            other => {
                self.clear_profile_data(context, reset)?;
                let reason = match other {
                    Err(error) => {
                        tracing::debug!(%error, "profile-data ownership unavailable");
                        UnavailableReason::from(&error)
                    }
                    _ => UnavailableReason::TargetNotReady,
                };
                self.data_unavailable(
                    context,
                    &Retry {
                        reason: reason.with_dependency("profile data"),
                        at: context.now.saturating_add(SAMPLE_INTERVAL),
                    },
                )?;
                return Ok(None);
            }
        };
        if self.profile_data.as_ref().is_some_and(|old| old != &owner) {
            self.clear_profile_data(context, reset)?;
            // Reset wakes consumers; cached layout failures still retain their deadlines.
            self.validate_due(context, image)?;
        }
        self.profile_data = Some(owner.clone());
        Ok(Some(owner))
    }

    pub(super) fn clear_profile_data(
        &mut self,
        context: &mut PollContext<'_>,
        reset: &mut bool,
    ) -> Result<(), ProviderError> {
        if self.profile_data.take().is_some() {
            self.clear_chat();
            if !*reset {
                for (cap, topic) in self.data_topics() {
                    if topic.demanded {
                        context.events.reset(cap)?;
                        topic.next_poll = context.now;
                    }
                }
                *reset = true;
            }
        }
        Ok(())
    }

    pub(super) fn data_unavailable(
        &mut self,
        context: &mut PollContext<'_>,
        retry: &Retry,
    ) -> Result<(), ProviderError> {
        for (cap, topic) in self.data_topics() {
            if topic.demanded {
                unavailable(context, cap, topic, retry.clone())?;
            }
        }
        Ok(())
    }
}
