use crate::{
    components::{Card, Notice},
    sdk::{Session, use_watch},
};
use dioxus::prelude::*;
use wf_observer_sdk::{Screen, ScreensState};

#[component]
pub fn Screens() -> Element {
    let Session(session) = use_context();
    let live = use_watch(
        move || {
            let session = session.clone();
            async move { Ok(session.screens().watch().await?.into_stream()) }
        },
        None,
    );
    rsx! {
        Card { title: "Screens", subtitle: "Visible game screens; several can be open together.",
            if let Some(problem) = (live.error)() { Notice { error: true, "{problem}" } }
            match (live.value)() {
                Some(ScreensState::Ready { value }) => rsx! {
                    if value.screens.is_empty() { p { class: "muted", "No visible screens" } }
                    ul {
                        for screen in value.screens {
                            li { "{label(&screen)}" }
                        }
                    }
                },
                Some(ScreensState::Unavailable { reason }) => rsx! { Notice { "Unavailable · {reason}" } },
                _ => rsx! { Notice { "Waiting for screens…" } },
            }
        }
    }
}

fn label(screen: &Screen) -> &str {
    match screen {
        Screen::Loadout => "Loadout",
        Screen::Navigation => "Navigation",
        Screen::Progress => "Progress",
        Screen::RelicRewards => "Relic rewards",
        Screen::Other { asset_path } => asset_path,
    }
}
