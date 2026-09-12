use crate::{
    components::{Button, Card, Notice},
    sdk::{Session, use_watch},
};
use dioxus::prelude::*;
use wf_observer_sdk::{CurrenciesState, WarframeCurrencies};

#[component]
pub fn Currencies() -> Element {
    let Session(session) = use_context();
    let watched_session = session.clone();
    let live = use_watch(
        move || {
            let session = watched_session.clone();
            async move { Ok(session.currencies().watch().await?.into_stream()) }
        },
        None,
    );
    let mut cached = use_signal(|| None::<WarframeCurrencies>);
    let mut cache_message = use_signal(|| None::<String>);
    let scope = use_memo(move || match live.value.read().as_ref() {
        Some(CurrenciesState::Ready { value }) => Some((
            value.metadata.source.clone(),
            value.metadata.generation.clone(),
            value.account_id.clone(),
        )),
        _ => None,
    });
    use_effect(move || {
        let _ = scope();
        cached.set(None);
        cache_message.set(None);
    });
    rsx! {
        Card { title: "Currencies", subtitle: "Complete balances, independently watched by this component.",
            if let Some(problem) = (live.error)() { Notice { error: true, "{problem}" } }
            match (live.value)() {
                Some(CurrenciesState::Ready { value }) => rsx! {
                    div { class: "balance-grid",
                        for (name, amount) in [
                            ("Credits", value.balances.credits), ("Endo", value.balances.endo),
                            ("Tradable platinum", value.balances.tradable_platinum),
                            ("Non-tradable platinum", value.balances.non_tradable_platinum),
                        ] { div { class: "balance", span { "{name}" } strong { "{amount}" } } }
                    }
                    details { class: "inspector", summary { "Account identity" } code { "{value.account_id}" } }
                },
                Some(CurrenciesState::Unavailable { reason }) => rsx! { Notice { "Unavailable · {reason}" } },
                _ => rsx! { Notice { "Waiting for balances…" } },
            }
            Button { secondary: true, disabled: scope().is_none(), onclick: move |_| {
                let Some(request_scope) = scope() else { return; };
                let session = session.clone();
                spawn(async move {
                    let result = session.currencies().cached().await;
                    if scope().as_ref() != Some(&request_scope) {
                        return;
                    }
                    cached.set(None);
                    cache_message.set(None);
                    match result {
                        Ok(Some(value)) if (
                            &value.metadata.source,
                            &value.metadata.generation,
                            &value.account_id,
                        ) == (&request_scope.0, &request_scope.1, &request_scope.2) => {
                            cached.set(Some(value));
                        }
                        Ok(_) => cache_message.set(Some("No cached sample for the current account".into())),
                        Err(problem) => cache_message.set(Some(problem.to_string())),
                    }
                });
            }, "Read service cache" }
            if let Some(value) = cached()
                && scope() == Some((value.metadata.source, value.metadata.generation, value.account_id)) {
                p { class: "small muted", "Cached credits: {value.balances.credits}" }
            }
            if let Some(message) = cache_message() { p { class: "small muted", "{message}" } }
        }
    }
}
