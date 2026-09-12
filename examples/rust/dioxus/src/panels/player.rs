use crate::{
    components::{Card, Notice},
    sdk::{Session, use_watch},
};
use dioxus::prelude::*;
use wf_observer_sdk::PlayerState;

#[component]
pub fn Player() -> Element {
    let Session(session) = use_context();
    let live = use_watch(
        move || {
            let session = session.clone();
            async move { Ok(session.player().watch().await?.into_stream()) }
        },
        None,
    );
    rsx! {
        Card { title: "Player", subtitle: "The logged-in player in this session.",
            if let Some(problem) = (live.error)() { Notice { error: true, "{problem}" } }
            match (live.value)() {
                Some(PlayerState::Ready { value }) => rsx! {
                    div { class: "player-identity", span { class: "avatar", "WF" }
                        div { h3 { "{value.username}" } p { class: "small muted", "Session {value.metadata.source.session.session_id}" } }
                    }
                    details { class: "inspector", summary { "Account identity" } code { "{value.account_id}" } }
                },
                Some(PlayerState::Unavailable { reason }) => rsx! { Notice { "Unavailable · {reason}" } },
                _ => rsx! { Notice { "Waiting for player data…" } },
            }
        }
    }
}
