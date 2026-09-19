//! Opt-in chat-only acquisition check; prints metadata but not message contents.
use fastant::Instant;
use provider_sdk::{
    CapabilityDescriptor, CapabilityHealth, EventSink, HealthSink, PollContext, Provider,
    ProviderError,
};
use provider_warframe::WarframeProvider;
use std::time::Duration;
use warframe_model::{ChatEvent, ChatUpdate};

#[derive(Default)]
struct Output {
    events: Vec<ChatEvent>,
    resets: usize,
    health: Vec<CapabilityHealth>,
}

impl EventSink for Output {
    fn reset(&mut self, cap: &CapabilityDescriptor) -> Result<(), ProviderError> {
        assert_eq!(cap.topic, "warframe.chat");
        self.resets += 1;
        Ok(())
    }
    fn snapshot(
        &mut self,
        _: &CapabilityDescriptor,
        _: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::Failed(
            "chat-only test received a snapshot".into(),
        ))
    }
    fn event(
        &mut self,
        cap: &CapabilityDescriptor,
        value: &serde_json::Value,
    ) -> Result<(), ProviderError> {
        assert_eq!(cap.topic, "warframe.chat");
        self.events.push(
            serde_json::from_value(value.clone())
                .map_err(|error| ProviderError::Failed(error.to_string()))?,
        );
        Ok(())
    }
}
impl HealthSink for Output {
    fn update(
        &mut self,
        cap: &CapabilityDescriptor,
        health: CapabilityHealth,
    ) -> Result<(), ProviderError> {
        assert_eq!(cap.topic, "warframe.chat");
        self.health.push(health);
        Ok(())
    }
}

#[test]
#[ignore = "requires a running logged-in game; send/receive whispers while it observes"]
fn current_game_chat() -> Result<(), Box<dyn std::error::Error>> {
    let provider = WarframeProvider;
    let targets = memory_reader::discover_targets_by(|metadata| {
        provider.identify_process(metadata).map(str::to_owned)
    })?;
    let [target] = targets.as_slice() else {
        return Err("expected exactly one Warframe process".into());
    };
    let mut memory = memory_reader::attach(target)?;
    let mut session = provider.start(target, &mut memory)?;
    let capability = provider
        .manifest()
        .capabilities
        .iter()
        .find(|cap| cap.topic == "warframe.chat")
        .ok_or("chat capability missing")?;
    session.begin_generation(capability);
    let seconds = std::env::var("WF_CHAT_TEST_SECONDS")
        .ok()
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(30);
    let started = Instant::now();
    let mut available = false;
    let mut messages = 0;
    println!("Observing only chat for {seconds} seconds; message contents are omitted.");
    loop {
        let mut events = Output::default();
        let mut health = Output::default();
        memory.verify()?;
        let result = session.poll(&mut PollContext {
            demand: &[capability],
            now: started.elapsed(),
            memory: &mut memory,
            events: &mut events,
            health: &mut health,
        });
        let verified = memory.verify();
        session.poll_completed(result.is_ok() && verified.is_ok());
        result?;
        verified?;
        for state in health.health {
            if state == CapabilityHealth::Available {
                if !available {
                    println!("Chat acquisition available; initial retained history baselined.");
                }
                available = true;
            } else {
                println!("Chat health: {state:?}");
            }
        }
        if events.resets != 0 {
            println!("Account scope reset; establishing a fresh baseline.");
        }
        for event in events.events {
            match event.update {
                ChatUpdate::Message { value } => {
                    assert!(!value.conversation_id.is_empty());
                    messages += 1;
                    println!(
                        "{:?} {:?}: conversation={}, sender={:?}, peer={:?}, time={:?}, bytes={}",
                        value.channel,
                        value.direction,
                        value.conversation_id,
                        value.sender,
                        value.peer,
                        value.game_time,
                        value.text.len()
                    );
                }
                ChatUpdate::Gap { channel } => {
                    println!("{channel:?}: skipped retained history after lost position");
                }
            }
        }
        if started.elapsed() >= Duration::from_secs(seconds) {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    assert!(available, "chat never became available");
    println!(
        "Observed {messages} live messages. Direction/peer correctness requires comparison with in-game authorship."
    );
    Ok(())
}
