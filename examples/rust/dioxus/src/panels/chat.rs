use crate::{
    components::{Card, Notice},
    sdk::{Session, health, use_watch},
};
use dioxus::prelude::*;
use std::collections::VecDeque;
use wf_observer_sdk::{CapabilityHealth, ChatObservation, ChatState};

const DISPLAY_MESSAGES: usize = 200;

#[component]
pub fn Chat() -> Element {
    let Session(session) = use_context();
    let mut messages = use_signal(VecDeque::<ChatObservation>::new);
    let mut source = use_signal(|| None::<ChatState>);
    let mut channel = use_signal(String::new);
    let on_item = use_callback(move |item: ChatObservation| match item {
        ChatObservation::State { value } => {
            if value.health != CapabilityHealth::Available
                || source.peek().as_ref().map(|s| &s.generation) != Some(&value.generation)
            {
                messages.write().clear();
            }
            source.set(Some(value));
        }
        event => {
            let mut rows = messages.write();
            if rows.len() == DISPLAY_MESSAGES {
                rows.pop_front();
            }
            rows.push_back(event);
        }
    });
    let live = use_watch(
        move || {
            let session = session.clone();
            async move { Ok(session.chat().watch().await?.into_stream()) }
        },
        Some(on_item),
    );
    use_effect(move || {
        if (live.value)().is_none() {
            messages.write().clear();
            source.set(None);
        }
    });
    let selected_channel = channel();
    let rows = messages.read();
    let channels = rows
        .iter()
        .map(channel_name)
        .collect::<std::collections::BTreeSet<_>>();
    let visible = rows
        .iter()
        .filter(|event| selected_channel.is_empty() || selected_channel == channel_name(event));
    rsx! {
        div { class: "wide-panel",
            Card { title: "Chat", subtitle: "New messages while mounted. Game-local clock time; markup is shown as text.",
                if let Some(problem) = (live.error)() { Notice { error: true, "{problem}" } }
                if let Some(state) = source() { p { class: "small muted", "{health(&state.health)}" } }
                div { class: "filters",
                    div { label { r#for: "chat-channel", "Channel" }
                        select { id: "chat-channel", value: channel(), onchange: move |event| channel.set(event.value()),
                            option { value: "", "All channels" }
                            for name in channels { option { value: name.clone(), "{name}" } }
                        }
                    }
                    span { class: "small muted", "Latest {DISPLAY_MESSAGES} messages in this view." }
                }
                div { class: "chat-log", role: "log", "aria-live": "polite", "aria-label": "Observed chat messages",
                    if rows.is_empty() { Notice { "Waiting for new chat. Earlier messages are not replayed." } }
                    for event in visible {
                        match event {
                            ChatObservation::Message { metadata, value: message, .. } => rsx! {
                                article { class: "chat-message", key: "{metadata.generation}:{metadata.sequence}",
                                    header {
                                        span { class: "channel", "{message.channel:?}" }
                                        strong { {message.sender.as_deref().unwrap_or("System")} }
                                        if let Some(time) = message.game_time { time { "{time.hour:02}:{time.minute:02}" } }
                                    }
                                    p { "{message.text}" }
                                }
                            },
                            ChatObservation::Gap { channel, .. } => rsx! { Notice { "{channel:?}: continuity lost. Following messages may overlap earlier ones." } },
                            ChatObservation::State { .. } => rsx! {},
                        }
                    }
                }
            }
        }
    }
}

fn channel_name(event: &ChatObservation) -> String {
    match event {
        ChatObservation::Message { value, .. } => format!("{:?}", value.channel),
        ChatObservation::Gap { channel, .. } => format!("{channel:?}"),
        ChatObservation::State { .. } => String::new(),
    }
}
