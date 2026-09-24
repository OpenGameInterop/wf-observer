use super::*;
use crate::session::fixture::{ACCOUNT, Memory, OTHER_ACCOUNT};
use provider_sdk::{EventSink, HealthSink};
use warframe_model::{RelicRewardPicker, RelicRewardsSnapshot, ScreensSnapshot};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const BASE: u64 = 0x1_4000_0000;
const CLIENT: u64 = BASE + 0x1040_0000;
const OVERLAY: u64 = CLIENT + 0x1_0000;
const FLASH: u64 = CLIENT + 0x2_0000;
const MOVIES: u64 = CLIENT + 0x3_0000;
const MOVIE: u64 = CLIENT + 0x4_0000;
const OWNER: u64 = CLIENT + 0x5_0000;
const PATH: u64 = CLIENT + 0x6_0000;
const CONTEXT: u64 = CLIENT + 0x7_0000;
const REGION: u64 = CLIENT + 0x8_0000;
const RULES: u64 = CLIENT + 0x9_0000;
const REWARDS: u64 = CLIENT + 0xa_0000;
const IDS: u64 = CLIENT + 0xb_0000;

fn image() -> Executable {
    Executable {
        base: BASE,
        actual: crate::target::BUILD,
    }
}

fn session() -> VisualTopics {
    VisualTopics {
        screen_layout: CachedCheck::Passed(()),
        relic_layout: CachedCheck::Passed(()),
        ..VisualTopics::default()
    }
}

fn memory(open: bool) -> Memory {
    let mut m = Memory::new(BASE);
    for (address, pointer) in [
        (BASE + 0x0272_fd80, CLIENT + 0x8000),
        (CLIENT + 0x8000, CLIENT),
        (CLIENT, BASE + 0x021c_a558),
        (CLIENT + 0x5e0, OVERLAY + 0x8000),
        (OVERLAY + 0x8000, OVERLAY),
        (OVERLAY, BASE + 0x0222_2300),
        (OVERLAY + 0x48, FLASH + 0x8000),
        (FLASH + 0x8000, FLASH),
        (FLASH, BASE + 0x0222_1518),
        (MOVIES, MOVIE + 0x8000),
        (MOVIE + 0x8000, MOVIE),
        (MOVIE, BASE + 0x0222_00f8),
        (MOVIE + 0x140, OWNER + 0x8000),
        (OWNER + 0x8000, OWNER),
        (MOVIE + 0x120, PATH),
        (CLIENT + 0x788, CONTEXT + 0x8000),
        (CONTEXT + 0x8000, CONTEXT),
        (CONTEXT, BASE + 0x021c_ecc0),
        (CONTEXT + 0x120, REGION + 0x8000),
        (REGION + 0x8000, REGION),
        (REGION, BASE + 0x021d_29d8),
        (BASE + 0x021d_29d8 + 0x300, BASE + 0x0119_8130),
        (REGION + 0x240, RULES + 0x8000),
        (RULES + 0x8000, RULES),
        (RULES, BASE + 0x0234_d910),
    ] {
        m.put(address, &pointer.to_le_bytes());
    }
    vector(&mut m, FLASH + 0xd0, MOVIES, 8);
    m.put(MOVIE + 0xfa, &[u8::from(open)]);
    m.put(MOVIE + 0x118, &[0]);
    m.put(MOVIE + 0x5440, &[0]);
    path(&mut m, "/Lotus/Interface/ProjectionRewardChoice.swf");
    vector(&mut m, RULES + 0x1870, REWARDS, 3 * 0x70);
    // Remote/local/remote memory order; the local and first remote share an item.
    for (i, (account, item)) in [
        (OTHER_ACCOUNT, 0x30_0000),
        (ACCOUNT, 0x30_0000),
        (b"000000000000000000000000", 0x31_0000),
    ]
    .into_iter()
    .enumerate()
    {
        let record = REWARDS + i as u64 * 0x70;
        m.put(record, &[0; 0x70]);
        for field in [0_u64, 0x10, 0x20, 0x30] {
            m.put(record + field + 15, &[15]);
        }
        let id = IDS + i as u64 * 0x100;
        m.put(id, account);
        m.put(record + 0x10, &id.to_le_bytes());
        m.put(record + 0x18, &24_u32.to_le_bytes());
        m.put(record + 0x1f, &[0xff]);
        m.put(record + 0x48, &(m.heap() + item).to_le_bytes());
        m.put(record + 0x50, &[1]);
    }
    m
}

fn vector(m: &mut Memory, at: u64, pointer: u64, length: u32) {
    m.put(at, &pointer.to_le_bytes());
    m.put(at + 8, &length.to_le_bytes());
    m.put(at + 12, &length.to_le_bytes());
}

fn path(m: &mut Memory, path: &str) {
    m.put(PATH, &[0; 256]);
    m.put(PATH, path.as_bytes());
}

#[derive(Default)]
struct Output {
    values: Vec<(&'static str, serde_json::Value)>,
    resets: Vec<&'static str>,
    health: Vec<(&'static str, CapabilityHealth)>,
}
impl EventSink for Output {
    fn reset(&mut self, cap: &CapabilityDescriptor) -> Result<(), ProviderError> {
        assert!(
            !self.resets.contains(&cap.topic),
            "at most one reset per topic per poll"
        );
        self.resets.push(cap.topic);
        Ok(())
    }
    fn snapshot(
        &mut self,
        cap: &CapabilityDescriptor,
        value: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        self.values.push((cap.topic, value.clone()));
        Ok(())
    }
    fn event(
        &mut self,
        _: &CapabilityDescriptor,
        _: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        unreachable!("snapshot topics")
    }
}
impl HealthSink for Output {
    fn update(
        &mut self,
        cap: &CapabilityDescriptor,
        health: CapabilityHealth,
    ) -> Result<(), ProviderError> {
        self.health.push((cap.topic, health));
        Ok(())
    }
}
impl Output {
    fn relic(&self) -> Result<RelicRewardsSnapshot, Box<dyn std::error::Error>> {
        Ok(serde_json::from_value(
            self.values
                .iter()
                .find(|(topic, _)| *topic == RELIC_REWARDS.topic)
                .ok_or("missing relic snapshot")?
                .1
                .clone(),
        )?)
    }
    fn screens(&self) -> Result<ScreensSnapshot, Box<dyn std::error::Error>> {
        Ok(serde_json::from_value(
            self.values
                .iter()
                .find(|(topic, _)| *topic == SCREENS.topic)
                .ok_or("missing screens snapshot")?
                .1
                .clone(),
        )?)
    }
}

fn poll(
    s: &mut VisualTopics,
    m: &mut Memory,
    demand: &[&'static CapabilityDescriptor],
    ms: u64,
) -> Result<Output, ProviderError> {
    let mut out = Output::default();
    let mut health = Output::default();
    let mut context = PollContext {
        demand,
        now: Duration::from_millis(ms),
        memory: m,
        events: &mut out,
        health: &mut health,
    };
    s.demand(&context);
    if s.due(context.now) {
        s.poll(
            &mut context,
            image(),
            &mut ItemTypeCache::default(),
            &mut StringTokenCache::default(),
            &mut SharedLayouts::validated(),
        )?;
    }
    out.health = health.health;
    Ok(out)
}

#[test]
fn screens_work_without_login_and_relic_consumers_share_the_same_source() -> TestResult {
    let mut m = memory(true);
    m.omit(BASE + 0x027a_53c0);
    let out = poll(&mut session(), &mut m, &[&SCREENS], 0)?;
    assert_eq!(out.screens()?.screens, [Screen::RelicRewards]);
    assert!(out.health.is_empty());
    let mut alone = memory(true);
    let mut both = memory(true);
    poll(&mut session(), &mut alone, &[&RELIC_REWARDS], 0)?.relic()?;
    poll(&mut session(), &mut both, &[&SCREENS, &RELIC_REWARDS], 0)?.relic()?;
    assert_eq!(alone.reads, both.reads);
    Ok(())
}

#[test]
fn relic_rewards_need_account_identity_but_not_profile_data() -> TestResult {
    let mut memory = memory(true);
    memory.omit(BASE + 0x0215_1328 + 0x390);
    memory.omit(memory.heap() + 0x4_0000 + 0x208);
    let output = poll(&mut session(), &mut memory, &[&SCREENS, &RELIC_REWARDS], 0)?;
    assert_eq!(output.screens()?.screens, [Screen::RelicRewards]);
    assert!(matches!(
        output.relic()?.picker,
        RelicRewardPicker::Open { .. }
    ));
    assert!(output.health.is_empty());
    Ok(())
}

#[test]
fn open_picker_is_initial_state_with_order_and_duplicate_choices_preserved() -> TestResult {
    let out = poll(&mut session(), &mut memory(true), &[&RELIC_REWARDS], 0)?;
    let RelicRewardPicker::Open { choices } = out.relic()?.picker else {
        return Err("picker closed".into());
    };
    let names: Vec<_> = choices
        .iter()
        .map(|choice| choice.item_key.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "/Lotus/Types/Items/MiscItems/AlloyPlate",
            "/Lotus/Types/Items/MiscItems/OrokinCell",
            "/Lotus/Types/Items/MiscItems/AlloyPlate"
        ]
    );
    Ok(())
}

#[test]
fn closed_picker_does_not_read_reward_records_and_idle_has_no_reads() -> TestResult {
    let mut s = session();
    let mut m = memory(false);
    m.omit(RULES + 0x1870);
    assert_eq!(
        poll(&mut s, &mut m, &[&RELIC_REWARDS], 0)?.relic()?.picker,
        RelicRewardPicker::Closed
    );
    assert!(!m.reads.contains(&(RULES + 0x1870)));
    m.reads.clear();
    poll(&mut s, &mut m, &[], 1)?;
    assert!(m.reads.is_empty());
    assert!(s.deadline().is_none());
    Ok(())
}

#[test]
fn visibility_filters_unknown_assets_and_malformed_flags_are_not_absence() -> TestResult {
    for at in [MOVIE + 0x118, MOVIE + 0x5440] {
        let mut m = memory(true);
        m.put(at, &[1]);
        assert!(
            poll(&mut session(), &mut m, &[&SCREENS], 0)?
                .screens()?
                .screens
                .is_empty()
        );
        m.put(at, &[2]);
        let out = poll(&mut session(), &mut m, &[&SCREENS], 0)?;
        assert!(out.values.is_empty());
        assert_eq!(out.health.len(), 1);
    }
    let mut m = memory(true);
    path(&mut m, "/Lotus/Interface/NewScreen.swf");
    vector(&mut m, FLASH + 0xd0, MOVIES, 16);
    m.put(MOVIES + 8, &(MOVIE + 0x8000).to_le_bytes());
    assert_eq!(
        poll(&mut session(), &mut m, &[&SCREENS], 0)?
            .screens()?
            .screens,
        [Screen::Other {
            asset_path: "/Lotus/Interface/NewScreen.swf".into()
        }]
    );
    m.put(FLASH + 0xd8, &2048_u32.to_le_bytes());
    assert!(
        poll(&mut session(), &mut m, &[&SCREENS], 0)?
            .values
            .is_empty()
    );
    Ok(())
}

#[test]
fn corrupt_rewards_and_layout_failures_do_not_poison_screens() -> TestResult {
    for malformed in [false, true] {
        let mut s = session();
        let mut m = memory(true);
        if malformed {
            m.put(REWARDS + 0x50, &[2]);
        } else {
            s.relic_layout = CachedCheck::Failed(Retry {
                reason: UnavailableReason::UnsupportedBuild,
                at: Duration::from_secs(5),
            });
        }
        let out = poll(&mut s, &mut m, &[&SCREENS, &RELIC_REWARDS], 0)?;
        assert_eq!(out.screens()?.screens, [Screen::RelicRewards]);
        assert_eq!(out.health.len(), 1);
        assert_eq!(out.health[0].0, RELIC_REWARDS.topic);
        m.reads.clear();
        s.wake(&RELIC_REWARDS);
        let out = poll(&mut s, &mut m, &[&SCREENS, &RELIC_REWARDS], 50)?;
        out.screens()?;
        if !malformed {
            assert!(!m.reads.contains(&(BASE + 0x014e_7907)));
        }
    }
    Ok(())
}

#[test]
fn closing_and_same_account_world_replacement_reset_only_relic_continuity() -> TestResult {
    let mut s = session();
    let mut m = memory(true);
    let demand = [&SCREENS, &RELIC_REWARDS];
    poll(&mut s, &mut m, &demand, 0)?.relic()?;
    m.put(MOVIE + 0xfa, &[0]);
    let out = poll(&mut s, &mut m, &demand, 50)?;
    assert_eq!(out.relic()?.picker, RelicRewardPicker::Closed);
    assert!(out.resets.is_empty());
    m.put(REGION + 0x240, &(RULES + 0x9000).to_le_bytes());
    m.put(RULES + 0x9000, &RULES.to_le_bytes());
    let out = poll(&mut s, &mut m, &demand, 100)?;
    assert_eq!(out.resets, [RELIC_REWARDS.topic]);
    out.relic()?;
    m.put(m.account(), OTHER_ACCOUNT);
    let out = poll(&mut s, &mut m, &demand, 150)?;
    assert_eq!(out.resets, [RELIC_REWARDS.topic]);
    assert_eq!(out.relic()?.account_id.as_str().as_bytes(), OTHER_ACCOUNT);
    Ok(())
}

#[test]
fn a_picker_that_settles_after_opening_recovers_on_the_next_screen_sample() -> TestResult {
    let mut s = session();
    let mut m = memory(false);
    let demand = [&SCREENS, &RELIC_REWARDS];
    assert_eq!(
        poll(&mut s, &mut m, &demand, 0)?.relic()?.picker,
        RelicRewardPicker::Closed
    );

    m.put(MOVIE + 0xfa, &[1]);
    m.put(REWARDS + 0x50, &[2]);
    let out = poll(&mut s, &mut m, &demand, 50)?;
    assert_eq!(out.screens()?.screens, [Screen::RelicRewards]);
    assert!(out.values.iter().all(|(topic, _)| *topic == SCREENS.topic));
    assert!(matches!(
        out.health.as_slice(),
        [(
            _,
            CapabilityHealth::Unavailable(UnavailableReason::DependencyUnavailable {
                failure: DependencyFailure::ValidationFailed,
                ..
            })
        )]
    ));

    m.put(REWARDS + 0x50, &[1]);
    let out = poll(&mut s, &mut m, &demand, 100)?;
    assert!(
        matches!(out.relic()?.picker, RelicRewardPicker::Open { choices } if choices.len() == 3)
    );
    assert!(out.health.is_empty());
    Ok(())
}

#[test]
fn screen_transitions_retry_at_the_fastest_requested_interval() -> TestResult {
    let public = [&SCREENS];
    let relic = [&RELIC_REWARDS];
    let both = [&SCREENS, &RELIC_REWARDS];
    for (demand, interval) in [
        (public.as_slice(), 250),
        (relic.as_slice(), 50),
        (both.as_slice(), 50),
    ] {
        let mut s = session();
        let mut m = memory(true);
        m.change_on_read(MOVIES, FLASH + 0xd8, &0_u32.to_le_bytes());
        let out = poll(&mut s, &mut m, demand, 0)?;
        assert!(out.values.is_empty());
        assert_eq!(out.health.len(), demand.len());
        assert!(out.health.iter().all(|(_, health)| {
            *health
                == CapabilityHealth::Unavailable(
                    UnavailableReason::TargetNotReady.with_dependency(SCREENS.topic),
                )
        }));

        m.reads.clear();
        assert!(
            poll(&mut s, &mut m, demand, interval - 1)?
                .values
                .is_empty()
        );
        assert!(m.reads.is_empty());
        let out = poll(&mut s, &mut m, demand, interval)?;
        assert_eq!(out.values.len(), demand.len());
        assert!(out.health.is_empty());
        if demand.iter().any(|cap| cap.topic == SCREENS.topic) {
            assert!(out.screens()?.screens.is_empty());
        }
        if demand.iter().any(|cap| cap.topic == RELIC_REWARDS.topic) {
            assert_eq!(out.relic()?.picker, RelicRewardPicker::Closed);
        }
    }
    Ok(())
}

#[test]
fn malformed_screen_data_keeps_the_slower_retry() -> TestResult {
    let mut s = session();
    let mut m = memory(true);
    let demand = [&SCREENS, &RELIC_REWARDS];
    m.put(MOVIE + 0xfa, &[2]);
    let out = poll(&mut s, &mut m, &demand, 0)?;
    assert!(out.values.is_empty());
    assert!(out.health.iter().all(|(_, health)| {
        matches!(
            health,
            CapabilityHealth::Unavailable(UnavailableReason::DependencyUnavailable {
                failure: DependencyFailure::ValidationFailed,
                ..
            })
        )
    }));
    m.put(MOVIE + 0xfa, &[1]);
    m.reads.clear();
    for ms in [50, 250, 999] {
        assert!(poll(&mut s, &mut m, &demand, ms)?.values.is_empty());
        assert!(m.reads.is_empty());
    }
    let out = poll(&mut s, &mut m, &demand, 1000)?;
    assert_eq!(out.screens()?.screens, [Screen::RelicRewards]);
    out.relic()?;
    assert!(out.health.is_empty());
    Ok(())
}

#[test]
fn the_hidden_flag_belongs_to_the_movie_not_its_owner() -> TestResult {
    let mut m = memory(true);
    m.put(OWNER + 0x5440, &[0xa5]);
    assert_eq!(
        poll(&mut session(), &mut m, &[&SCREENS], 0)?
            .screens()?
            .screens,
        [Screen::RelicRewards]
    );
    assert!(!m.reads.contains(&(OWNER + 0x5440)));
    m.put(MOVIE + 0x5440, &[1]);
    m.put(OWNER + 0x5440, &[0]);
    assert!(
        poll(&mut session(), &mut m, &[&SCREENS], 0)?
            .screens()?
            .screens
            .is_empty()
    );
    Ok(())
}

#[test]
fn pending_reward_items_keep_fast_checks_without_publishing_partial_choices() -> TestResult {
    let mut s = session();
    let mut m = memory(true);
    let demand = [&SCREENS, &RELIC_REWARDS];
    let item_at = REWARDS + 2 * 0x70 + 0x48;
    m.put(item_at, &0_u64.to_le_bytes());
    for ms in (0..500).step_by(50) {
        m.reads.clear();
        let out = poll(&mut s, &mut m, &demand, ms)?;
        assert_eq!(out.screens()?.screens, [Screen::RelicRewards]);
        assert!(out.values.iter().all(|(topic, _)| *topic == SCREENS.topic));
        assert!(m.reads.contains(&REWARDS));
        assert!(matches!(
            out.health.as_slice(),
            [(
                _,
                CapabilityHealth::Unavailable(UnavailableReason::DependencyUnavailable {
                    failure: DependencyFailure::TargetNotReady,
                    ..
                })
            )]
        ));
    }
    m.put(item_at, &(m.heap() + 0x31_0000).to_le_bytes());
    let out = poll(&mut s, &mut m, &demand, 500)?;
    assert!(
        matches!(out.relic()?.picker, RelicRewardPicker::Open { choices } if choices.len() == 3)
    );
    assert!(out.health.is_empty());
    Ok(())
}

#[test]
fn persistent_reward_failures_back_off_without_delaying_screens_and_success_resets_backoff()
-> TestResult {
    let mut s = session();
    let mut m = memory(true);
    let demand = [&SCREENS, &RELIC_REWARDS];
    m.put(REWARDS + 0x50, &[2]);
    let mut attempts = Vec::new();
    for ms in (0..=3500).step_by(50) {
        m.reads.clear();
        let out = poll(&mut s, &mut m, &demand, ms)?;
        out.screens()?;
        assert!(out.values.iter().all(|(topic, _)| *topic == SCREENS.topic));
        if m.reads.contains(&REWARDS) {
            attempts.push(ms);
        }
    }
    assert_eq!(attempts, [0, 50, 150, 350, 750, 1550, 2550]);

    m.put(REWARDS + 0x50, &[1]);
    poll(&mut s, &mut m, &demand, 3550)?.relic()?;
    m.put(REWARDS + 0x50, &[2]);
    assert!(poll(&mut s, &mut m, &demand, 3600)?.relic().is_err());
    m.put(REWARDS + 0x50, &[1]);
    poll(&mut s, &mut m, &demand, 3650)?.relic()?;
    Ok(())
}

#[test]
fn fast_reward_retries_do_not_shorten_layout_validation_deadlines() -> TestResult {
    let mut s = session();
    let mut m = memory(true);
    let demand = [&SCREENS, &RELIC_REWARDS];
    s.relic_layout = CachedCheck::Failed(Retry {
        reason: UnavailableReason::UnsupportedBuild,
        at: Duration::from_secs(5),
    });
    for ms in (0..5000).step_by(50) {
        m.reads.clear();
        let out = poll(&mut s, &mut m, &demand, ms)?;
        out.screens()?;
        assert!(out.values.iter().all(|(topic, _)| *topic == SCREENS.topic));
        assert!(!m.reads.contains(&REWARDS));
        assert!(!m.reads.contains(&(BASE + 0x014e_7907)));
    }
    s.relic_layout = CachedCheck::Passed(());
    poll(&mut s, &mut m, &demand, 5000)?.relic()?;
    Ok(())
}

#[test]
fn ownership_or_visibility_changes_mid_read_discard_rewards() -> TestResult {
    for visibility in [false, true] {
        let mut s = session();
        let mut m = memory(true);
        let demand = [&SCREENS, &RELIC_REWARDS];
        poll(&mut s, &mut m, &demand, 0)?.relic()?;
        if visibility {
            m.change_on_read(REWARDS, MOVIE + 0xfa, &[0]);
        } else {
            m.change_on_read(REWARDS, m.account(), OTHER_ACCOUNT);
        }
        let out = poll(&mut s, &mut m, &demand, 50)?;
        assert!(out.values.iter().all(|(topic, _)| *topic == SCREENS.topic));
        assert_eq!(out.resets, [RELIC_REWARDS.topic]);
        assert!(
            matches!(out.health.as_slice(), [(topic, CapabilityHealth::Unavailable(UnavailableReason::DependencyUnavailable { failure: DependencyFailure::TargetNotReady, .. }))] if *topic == RELIC_REWARDS.topic)
        );
    }
    Ok(())
}

#[test]
fn account_order_changes_between_full_samples_are_rejected() -> TestResult {
    let mut m = memory(true);
    // Each record's account string is read twice. Change a remote ID in the
    // second complete vector sample, leaving reward pointers unchanged.
    m.change_on_nth_read(REWARDS, 3, IDS, b"111111111111111111111111");
    let out = poll(&mut session(), &mut m, &[&RELIC_REWARDS], 0)?;
    assert!(out.values.is_empty());
    assert!(matches!(
        out.health.as_slice(),
        [(
            _,
            CapabilityHealth::Unavailable(UnavailableReason::DependencyUnavailable {
                failure: DependencyFailure::TargetNotReady,
                ..
            })
        )]
    ));
    Ok(())
}

#[test]
fn reward_vector_bounds_and_ambiguous_account_fields_fail_before_publication() -> TestResult {
    for case in 0..7 {
        let mut m = memory(true);
        match case {
            0 => m.put(RULES + 0x1878, &(5_u32 * 0x70).to_le_bytes()),
            1 => m.put(RULES + 0x187c, &(65_u32 * 0x70).to_le_bytes()),
            2 => m.put(RULES + 0x1878, &113_u32.to_le_bytes()),
            3 => m.put(REWARDS + 0x48, &3_u64.to_le_bytes()),
            4 => m.put(IDS, ACCOUNT), // Duplicate account, not a duplicate item.
            5 => m.put(IDS + 0x100, b"111111111111111111111111"), // No local choice.
            _ => {
                for index in 0..3_u64 {
                    let record = REWARDS + index * 0x70;
                    m.put(record, &(IDS + index * 0x100).to_le_bytes());
                    m.put(record + 8, &24_u32.to_le_bytes());
                    m.put(record + 15, &[0xff]);
                }
            }
        }
        let out = poll(&mut session(), &mut m, &[&RELIC_REWARDS], 0)?;
        assert!(out.values.is_empty(), "case {case}");
        assert_eq!(out.health.len(), 1);
    }
    Ok(())
}

#[test]
fn unqualified_entries_and_empty_open_transition_are_valid() -> TestResult {
    let mut m = memory(true);
    for index in 0..3_u64 {
        m.put(REWARDS + index * 0x70 + 0x50, &[0]);
    }
    assert_eq!(
        poll(&mut session(), &mut m, &[&RELIC_REWARDS], 0)?
            .relic()?
            .picker,
        RelicRewardPicker::Open { choices: vec![] }
    );
    vector(&mut m, RULES + 0x1870, 0, 0);
    assert_eq!(
        poll(&mut session(), &mut m, &[&RELIC_REWARDS], 0)?
            .relic()?
            .picker,
        RelicRewardPicker::Open { choices: vec![] }
    );
    Ok(())
}

#[test]
fn a_second_owner_change_in_one_poll_does_not_emit_a_second_reset() -> TestResult {
    let mut s = session();
    let mut m = memory(true);
    poll(&mut s, &mut m, &[&RELIC_REWARDS], 0)?.relic()?;
    m.put(m.account(), OTHER_ACCOUNT);
    m.change_on_read(REWARDS, m.account(), ACCOUNT);
    let out = poll(&mut s, &mut m, &[&RELIC_REWARDS], 50)?;
    assert_eq!(out.resets, [RELIC_REWARDS.topic]);
    assert!(out.values.is_empty());
    Ok(())
}
