use crate::{
    components::{Card, Notice},
    sdk::{Session, use_watch},
};
use dioxus::prelude::*;
use wf_observer_sdk::{RelicRewardPicker, RelicRewardsState};

#[component]
pub fn RelicRewards() -> Element {
    let Session(session) = use_context();
    let live = use_watch(
        move || {
            let session = session.clone();
            async move { Ok(session.relic_rewards().watch().await?.into_stream()) }
        },
        None,
    );
    rsx! {
        Card { title: "Relic rewards", subtitle: "The current picker, in display order.",
            if let Some(problem) = (live.error)() { Notice { error: true, "{problem}" } }
            match (live.value)() {
                Some(RelicRewardsState::Ready { value }) => match value.picker {
                    RelicRewardPicker::Closed => rsx! { p { class: "muted", "Picker closed" } },
                    RelicRewardPicker::Open { choices } => rsx! {
                        if choices.is_empty() { p { class: "muted", "Picker opening…" } }
                        ol { for choice in choices { li { code { "{choice.item_key}" } } } }
                    },
                },
                Some(RelicRewardsState::Unavailable { reason }) => rsx! { Notice { "Unavailable · {reason}" } },
                _ => rsx! { Notice { "Waiting for relic picker state…" } },
            }
        }
    }
}
