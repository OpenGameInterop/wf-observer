//! Coherent chat acquisition through the real session, using synthetic native lists.
use super::*;
use crate::session::fixture::{Memory, OTHER_ACCOUNT};
use provider_sdk::{EventSink, HealthSink};
use warframe_model::{ChatDirection, ChatMessage, ChatUpdate};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const BASE: u64 = 0x1_4000_0000;

fn session() -> WarframeSession {
    let mut session = WarframeSession {
        executable: CachedCheck::Passed(Executable {
            base: BASE,
            actual: crate::target::BUILD,
        }),
        layouts: SharedLayouts::validated(),
        ..WarframeSession::default()
    };
    session.chat.layout = CachedCheck::Passed(());
    session.player.layout = CachedCheck::Passed(());
    session
}

fn string(memory: &mut Memory, at: u64, value: &str) {
    assert!(value.len() <= 15);
    let mut bytes = [0; 16];
    bytes[..value.len()].copy_from_slice(value.as_bytes());
    bytes[15] = u8::try_from(15 - value.len()).unwrap_or_default();
    memory.put(at, &bytes);
}

fn local_name(memory: &mut Memory, value: &str) {
    memory.put(BASE + 0x0211_ddc8 + 8, &(BASE + 0x0146_0580).to_le_bytes());
    string(memory, memory.heap() + 0x4_0000 + 0x50, value);
    string(memory, memory.heap() + 0x4_0000 + 0x40, "");
}

fn history(memory: &mut Memory, key: &str, entries: &[(u64, &str, &str)]) {
    // Explicit fixture offsets do not follow production facts.
    let head = memory.data() + 0x11ea8;
    let channel = memory.heap() + 0x40_0000;
    let sentinel = channel + 0x28;
    let node = |id| channel + 0x100 + id * 0x100;
    memory.put(
        BASE + 0x0234_c958 + 0x248,
        &(BASE + 0x00f0_f100).to_le_bytes(),
    );
    for (at, value) in [
        (head, channel),
        (head + 8, channel),
        (channel, head),
        (channel + 8, head),
        (
            sentinel,
            entries.first().map_or(sentinel, |entry| node(entry.0)),
        ),
        (
            sentinel + 8,
            entries.last().map_or(sentinel, |entry| node(entry.0)),
        ),
    ] {
        memory.put(at, &value.to_le_bytes());
    }
    string(memory, channel + 0x18, key);
    for (index, &(id, sender, text)) in entries.iter().enumerate() {
        let at = node(id);
        let previous = index
            .checked_sub(1)
            .map_or(sentinel, |i| node(entries[i].0));
        let next = entries
            .get(index + 1)
            .map_or(sentinel, |entry| node(entry.0));
        memory.put(at, &next.to_le_bytes());
        memory.put(at + 8, &previous.to_le_bytes());
        string(memory, at + 0x18, sender);
        string(memory, at + 0x28, text);
        string(
            memory,
            at + 0x38,
            if sender.is_empty() { "" } else { "[12:34] " },
        );
        memory.put(at + 0x48, &3_u32.to_le_bytes()); // Operator is also a player role.
    }
}

fn memory() -> Memory {
    let mut memory = Memory::new(BASE);
    local_name(&mut memory, "Me\u{e000}");
    history(&mut memory, "Alice,Me", &[]);
    memory
}

#[derive(Default)]
struct Output {
    events: Vec<ChatEvent>,
    resets: usize,
    health: Vec<CapabilityHealth>,
    reject: bool,
}

impl EventSink for Output {
    fn reset(&mut self, cap: &CapabilityDescriptor) -> Result<(), ProviderError> {
        assert_eq!(cap.topic, CHAT.topic);
        self.resets += 1;
        Ok(())
    }

    fn snapshot(
        &mut self,
        _: &CapabilityDescriptor,
        _: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::Failed(
            "chat must not publish a player snapshot".into(),
        ))
    }

    fn event(
        &mut self,
        cap: &CapabilityDescriptor,
        payload: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        assert_eq!(cap.topic, CHAT.topic);
        if self.reject {
            return Err(ProviderError::Failed("test publication rejection".into()));
        }
        self.events.push(
            serde_json::from_value(payload.clone())
                .map_err(|error| ProviderError::Failed(error.to_string()))?,
        );
        Ok(())
    }
}

impl HealthSink for Output {
    fn update(
        &mut self,
        cap: &CapabilityDescriptor,
        value: CapabilityHealth,
    ) -> Result<(), ProviderError> {
        assert_eq!(
            cap.topic, CHAT.topic,
            "optional name acquisition must not publish player health"
        );
        self.health.push(value);
        Ok(())
    }
}

fn poll(
    session: &mut WarframeSession,
    memory: &mut Memory,
    second: u64,
    committed: bool,
) -> TestResult<Output> {
    let mut events = Output::default();
    let mut health = Output::default();
    let result = session.poll(&mut PollContext {
        demand: &[&CHAT],
        now: Duration::from_secs(second),
        memory,
        events: &mut events,
        health: &mut health,
    });
    session.poll_completed(committed && result.is_ok());
    result?;
    events.health = health.health;
    Ok(events)
}

fn message(output: &Output) -> TestResult<&ChatMessage> {
    let [
        ChatEvent {
            update: ChatUpdate::Message { value },
            ..
        },
    ] = output.events.as_slice()
    else {
        return Err(format!("expected one message, got {:?}", output.events).into());
    };
    Ok(value)
}

#[test]
fn replacing_profile_data_discards_pending_chat_even_for_the_same_account() -> TestResult {
    let mut session = session();
    let mut memory = memory();
    poll(&mut session, &mut memory, 0, true)?;
    history(&mut memory, "Alice,Me", &[(1, "Alice", "old")]);
    assert_eq!(
        message(&poll(&mut session, &mut memory, 1, false)?)?.text,
        "old"
    );
    let replacement = memory.heap() + 0x7_0000;
    memory.put(replacement, &memory.data().to_le_bytes());
    memory.put(memory.heap() + 0x4_0000 + 0x208, &replacement.to_le_bytes());
    let reset = poll(&mut session, &mut memory, 2, true)?;
    assert_eq!(reset.resets, 1);
    assert!(
        reset.events.is_empty(),
        "retained history must become a new baseline"
    );
    history(
        &mut memory,
        "Alice,Me",
        &[(1, "Alice", "old"), (2, "Alice", "new")],
    );
    assert_eq!(
        message(&poll(&mut session, &mut memory, 3, true)?)?.text,
        "new"
    );
    Ok(())
}

#[test]
fn chat_only_reads_local_identity_and_preserves_authors_peers_and_ids() -> TestResult {
    let mut memory = memory();
    let mut session = session();
    history(&mut memory, "Alice,Me", &[(1, "Alice", "old history")]);
    assert!(poll(&mut session, &mut memory, 0, true)?.events.is_empty());
    history(
        &mut memory,
        "Alice,Me",
        &[(1, "Alice", "old history"), (2, "Alice", "hello")],
    );
    let incoming = poll(&mut session, &mut memory, 1, true)?;
    let first = message(&incoming)?;
    assert_eq!(first.direction, ChatDirection::Incoming);
    assert_eq!(first.sender.as_deref(), Some("Alice"));
    assert_eq!(first.peer.as_deref(), Some("Alice"));
    history(
        &mut memory,
        "Alice,Me",
        &[
            (1, "Alice", "old history"),
            (2, "Alice", "hello"),
            (3, "Me\u{e000}", "reply"),
        ],
    );
    let outgoing = poll(&mut session, &mut memory, 2, true)?;
    let reply = message(&outgoing)?;
    assert_eq!(reply.direction, ChatDirection::Outgoing);
    assert_eq!(reply.sender.as_deref(), Some("Me"));
    assert_eq!(reply.peer, first.peer);
    assert_eq!(reply.conversation_id, first.conversation_id);
    assert!(!session.player.demanded);
    Ok(())
}

#[test]
fn optional_identity_failures_leave_chat_available_and_do_not_reclassify_history() -> TestResult {
    for layout_failure in [false, true] {
        let mut memory = memory();
        let mut session = session();
        poll(&mut session, &mut memory, 0, true)?;
        if layout_failure {
            session.player.layout = CachedCheck::Failed(Retry {
                reason: UnavailableReason::UnsupportedBuild,
                at: Duration::from_secs(10),
            });
        } else {
            memory.omit(memory.heap() + 0x4_0000 + 0x50);
        }
        history(&mut memory, "Alice,Me", &[(1, "Alice", "hello")]);
        let output = poll(&mut session, &mut memory, 1, true)?;
        assert_eq!(output.health, [CapabilityHealth::Available]);
        assert_eq!(message(&output)?.direction, ChatDirection::Unknown);
        assert_eq!(message(&output)?.peer, None);
        local_name(&mut memory, "Me");
        session.player.layout = CachedCheck::Passed(());
        assert!(poll(&mut session, &mut memory, 2, true)?.events.is_empty());
    }
    Ok(())
}

#[test]
fn empty_system_channel_is_readable_without_a_player_identity() -> TestResult {
    let mut memory = memory();
    let mut session = session();
    history(&mut memory, "", &[]);
    local_name(&mut memory, "");
    poll(&mut session, &mut memory, 0, true)?;
    history(&mut memory, "", &[(1, "", "system notice")]);
    let output = poll(&mut session, &mut memory, 1, true)?;
    let value = message(&output)?;
    assert_eq!(value.direction, ChatDirection::System);
    assert_eq!(value.sender, None);
    assert_eq!(value.peer, None);
    assert_eq!(value.game_time, None);
    assert!(!value.conversation_id.is_empty());
    Ok(())
}

#[test]
fn rejected_publication_survives_native_trimming_and_commits_only_after_acceptance() -> TestResult {
    let mut memory = memory();
    let mut session = session();
    poll(&mut session, &mut memory, 0, true)?;
    history(&mut memory, "Alice,Me", &[(1, "Alice", "first")]);
    let rejected = poll(&mut session, &mut memory, 1, false)?;
    assert!(session.pending_chat.is_some());
    history(&mut memory, "Alice,Me", &[(2, "Alice", "trimmed tail")]);
    let retried = poll(&mut session, &mut memory, 2, true)?;
    assert_eq!(retried.events, rejected.events);
    assert!(session.pending_chat.is_none());
    let rebased = poll(&mut session, &mut memory, 3, true)?;
    assert!(matches!(
        rebased.events.as_slice(),
        [ChatEvent {
            update: ChatUpdate::Gap { .. },
            ..
        }]
    ));
    history(
        &mut memory,
        "Alice,Me",
        &[(2, "Alice", "trimmed tail"), (3, "Alice", "new")],
    );
    assert_eq!(
        message(&poll(&mut session, &mut memory, 4, true)?)?.text,
        "new"
    );
    Ok(())
}

#[test]
fn sink_failure_retries_the_prepared_batch_and_unrelated_completion_cannot_commit_it() -> TestResult
{
    let mut memory = memory();
    let mut session = session();
    poll(&mut session, &mut memory, 0, true)?;
    history(&mut memory, "Alice,Me", &[(1, "Alice", "first")]);
    let mut events = Output {
        reject: true,
        ..Output::default()
    };
    let mut health = Output::default();
    assert!(
        session
            .poll(&mut PollContext {
                demand: &[&CHAT],
                now: Duration::from_secs(1),
                memory: &mut memory,
                events: &mut events,
                health: &mut health,
            })
            .is_err()
    );
    session.poll_completed(false);
    // No chat publication was staged in this separate completion.
    session.poll_completed(true);
    assert!(session.pending_chat.is_some());
    history(&mut memory, "Alice,Me", &[]);
    assert_eq!(
        message(&poll(&mut session, &mut memory, 2, true)?)?.text,
        "first"
    );
    Ok(())
}

#[test]
fn account_replacement_during_acquisition_discards_enrichment_and_old_tracking() -> TestResult {
    let mut memory = memory();
    let mut session = session();
    poll(&mut session, &mut memory, 0, true)?;
    history(&mut memory, "Alice,Me", &[(1, "Me", "old account")]);
    memory.change_on_read(
        memory.heap() + 0x40_0000 + 0x200 + 0x28,
        memory.account(),
        OTHER_ACCOUNT,
    );
    let output = poll(&mut session, &mut memory, 1, true)?;
    assert!(output.events.is_empty());
    assert_eq!(output.resets, 1);
    assert!(session.pending_chat.is_none());
    assert!(session.chat_history.is_none());
    local_name(&mut memory, "Other");
    assert!(poll(&mut session, &mut memory, 2, true)?.events.is_empty());
    history(
        &mut memory,
        "Alice,Me",
        &[(1, "Me", "old account"), (2, "Me", "new sample")],
    );
    let output = poll(&mut session, &mut memory, 3, true)?;
    assert_eq!(message(&output)?.direction, ChatDirection::Incoming);
    assert_eq!(message(&output)?.peer, None);
    assert_eq!(
        output.events[0].account_id.as_str().as_bytes(),
        OTHER_ACCOUNT
    );
    Ok(())
}

#[test]
fn account_and_generation_changes_discard_rejected_batches_and_baseline_again() -> TestResult {
    for account_change in [false, true] {
        let mut memory = memory();
        let mut session = session();
        poll(&mut session, &mut memory, 0, true)?;
        history(&mut memory, "Alice,Me", &[(1, "Alice", "old pending")]);
        poll(&mut session, &mut memory, 1, false)?;
        if account_change {
            memory.put(memory.account(), OTHER_ACCOUNT);
            local_name(&mut memory, "Other");
        } else {
            session.begin_generation(&CHAT);
        }
        assert!(poll(&mut session, &mut memory, 2, true)?.events.is_empty());
        assert!(session.pending_chat.is_none());
    }
    Ok(())
}
