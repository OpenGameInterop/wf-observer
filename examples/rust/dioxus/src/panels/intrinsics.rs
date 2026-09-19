use crate::{
    components::{Card, Notice},
    sdk::{Session, use_watch},
};
use dioxus::prelude::*;
use wf_observer_sdk::IntrinsicsState;

#[component]
pub fn Intrinsics() -> Element {
    let Session(session) = use_context();
    let live = use_watch(
        move || {
            let session = session.clone();
            async move { Ok(session.intrinsics().watch().await?.into_stream()) }
        },
        None,
    );
    let current = (live.value)();
    rsx! {
        Card { title: "Intrinsics", subtitle: "Purchased ranks and whole points available to spend.",
            if let Some(problem) = (live.error)() { Notice { error: true, "{problem}" } }
            match &current {
                Some(IntrinsicsState::Unavailable { reason }) => rsx! { Notice { "Unavailable · {reason}" } },
                Some(IntrinsicsState::Ready { value }) => rsx! {
                    h3 { "Railjack · {value.railjack.unspent_points} unspent" }
                    div { class: "balance-grid",
                        for (name, rank) in [
                            ("Piloting", value.railjack.piloting), ("Gunnery", value.railjack.gunnery),
                            ("Tactical", value.railjack.tactical), ("Engineering", value.railjack.engineering),
                            ("Command", value.railjack.command),
                        ] { div { class: "balance", span { "{name}" } strong { "{rank} / 10" } } }
                    }
                    h3 { "Drifter · {value.drifter.unspent_points} unspent" }
                    div { class: "balance-grid",
                        for (name, rank) in [
                            ("Combat", value.drifter.combat), ("Riding", value.drifter.riding),
                            ("Opportunity", value.drifter.opportunity), ("Endurance", value.drifter.endurance),
                        ] { div { class: "balance", span { "{name}" } strong { "{rank} / 10" } } }
                    }
                },
                _ => rsx! { Notice { "Waiting for Intrinsics…" } },
            }
        }
    }
}
