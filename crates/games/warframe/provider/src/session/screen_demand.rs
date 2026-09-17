//! Provider-private dependencies, rebuilt from current demand on every poll.
//! No subscriptions, callbacks, clocks or detached tasks are created here.

use std::time::Duration;
use warframe_model::{Screen, ScreensSnapshot};

#[derive(Debug, ..Copy, ..Eq)]
pub(super) enum Consumer {
    Screens,
    RelicRewards,
}

#[derive(Debug, Clone, ..Eq)]
pub(super) enum ScreenCondition {
    Any,
    Visible(Screen),
}

#[derive(Debug, Clone, ..Eq)]
pub(super) struct ScreenRequirement {
    pub(super) condition: ScreenCondition,
    pub(super) preferred_interval: Duration,
}

#[derive(Default, ..Eq)]
pub(super) struct SourcePlan {
    requests: Vec<(Consumer, ScreenRequirement)>,
}

impl SourcePlan {
    pub(super) fn require_screens(&mut self, consumer: Consumer, requirement: ScreenRequirement) {
        self.requests.push((consumer, requirement));
    }

    fn interval(&self) -> Option<Duration> {
        self.requests
            .iter()
            .map(|(_, request)| request.preferred_interval)
            .min()
    }

    pub(super) fn matches(&self, consumer: Consumer, value: &ScreensSnapshot) -> bool {
        self.requests.iter().any(|(who, request)| {
            *who == consumer
                && match &request.condition {
                    ScreenCondition::Any => true,
                    ScreenCondition::Visible(screen) => value.screens.contains(screen),
                }
        })
    }
}

#[derive(Default)]
pub(super) struct ScreenSource {
    pub(super) plan: SourcePlan,
    next: Option<Duration>,
    last: Option<Duration>,
}

impl ScreenSource {
    pub(super) fn update(&mut self, plan: SourcePlan, now: Duration) {
        let interval = plan.interval();
        if interval.is_none() {
            self.next = None;
            self.last = None;
        } else if plan != self.plan {
            // New consumers need a current baseline; faster requests must not
            // sit behind an earlier slow deadline. Removing demand can slow us.
            let added = plan
                .requests
                .iter()
                .any(|request| !self.plan.requests.contains(request));
            self.next = Some(if added {
                now
            } else {
                self.last
                    .unwrap_or(now)
                    .saturating_add(interval.unwrap_or_default())
                    .max(now)
            });
        }
        self.plan = plan;
    }

    pub(super) fn wake(&mut self) {
        if self.next.is_some() {
            self.next = Some(Duration::ZERO);
        }
    }

    pub(super) fn due(&self, now: Duration) -> bool {
        self.next.is_some_and(|at| now >= at)
    }

    pub(super) fn deadline(&self) -> Option<Duration> {
        self.next
    }

    pub(super) fn sampled(&mut self, now: Duration) {
        self.last = Some(now);
        self.next = self
            .plan
            .interval()
            .map(|interval| now.saturating_add(interval));
    }

    pub(super) fn defer(&mut self, until: Duration) {
        self.next = Some(until);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(public: bool, relic: bool) -> SourcePlan {
        let mut plan = SourcePlan::default();
        if public {
            plan.require_screens(
                Consumer::Screens,
                ScreenRequirement {
                    condition: ScreenCondition::Any,
                    preferred_interval: Duration::from_millis(250),
                },
            );
        }
        if relic {
            plan.require_screens(
                Consumer::RelicRewards,
                ScreenRequirement {
                    condition: ScreenCondition::Visible(Screen::RelicRewards),
                    preferred_interval: Duration::from_millis(50),
                },
            );
        }
        plan
    }

    #[test]
    fn fastest_request_wakes_source_and_removal_slows_or_stops_it() {
        let mut source = ScreenSource::default();
        source.update(plan(true, false), Duration::ZERO);
        source.sampled(Duration::ZERO);
        assert_eq!(source.deadline(), Some(Duration::from_millis(250)));
        let now = Duration::from_millis(10);
        source.update(plan(true, true), now);
        assert!(source.due(now));
        source.sampled(now);
        assert_eq!(source.deadline(), Some(Duration::from_millis(60)));
        source.update(plan(true, false), now);
        assert_eq!(source.deadline(), Some(Duration::from_millis(260)));
        source.update(plan(false, false), now);
        assert_eq!(source.deadline(), None);
    }

    #[test]
    fn requirements_match_current_state_including_an_already_open_picker() {
        let plan = plan(true, true);
        let mut value = ScreensSnapshot { screens: vec![] };
        assert!(plan.matches(Consumer::Screens, &value));
        assert!(!plan.matches(Consumer::RelicRewards, &value));
        value.screens.push(Screen::RelicRewards);
        assert!(plan.matches(Consumer::RelicRewards, &value));
    }
}
