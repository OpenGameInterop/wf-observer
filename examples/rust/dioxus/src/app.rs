use dioxus::prelude::*;
use std::sync::Arc;
use wf_observer_sdk::{self as types, connect};

use crate::{
    components::{Button, Card, Input, Notice},
    panels::{Panels, Service},
    sdk::{Connection, Session, request},
};

const STYLES: &str = include_str!("../assets/app.css");

#[component]
pub fn App() -> Element {
    let mut endpoint = use_signal(String::new);
    let mut connection = use_signal(|| None::<Connection>);
    let mut error = use_signal(|| None::<String>);
    let mut connecting = use_signal(|| false);
    rsx! {
        document::Title { "WF Observer · SDK showcase" }
        document::Style { "{STYLES}" }
        div { class: "app-shell",
            header { class: "masthead",
                div { class: "brand-mark", "W" }
                div { h1 { "WF Observer" } p { "SDK SHOWCASE / DIOXUS" } }
                span { class: "connection-badge", if connection().is_some() { "Connected" } else { "Disconnected" } }
            }
            if let Some(current) = connection() {
                Connected { connection: current, on_disconnect: move |()| {
                    if let Some(Connection(client)) = connection.take() {
                        spawn(async move { let _ = client.shutdown().await; });
                    }
                } }
            } else {
                main { class: "welcome",
                    div { class: "welcome-copy",
                        p { class: "eyebrow", "ONE CONNECTION. INDEPENDENT COMPONENTS." }
                        h2 { "Your game,\nin view." }
                        p { "Explore live player data, currency balances, inventory and chat through the Rust SDK." }
                        div { class: "topic-chips", for name in ["Player", "Currencies", "Inventory", "Chat"] { span { "{name}" } } }
                    }
                    Card { title: "Connect to your observer", subtitle: "Start the service, then copy the endpoint ID from wf-observer status.",
                        label { r#for: "endpoint", "Endpoint ID or ticket" }
                        Input { id: "endpoint", value: endpoint(), placeholder: "Paste your endpoint ID or ticket", oninput: move |event: FormEvent| endpoint.set(event.value()) }
                        Button { disabled: connecting() || endpoint().trim().is_empty(), onclick: move |_| {
                            connecting.set(true);
                            error.set(None);
                            spawn(async move {
                                let text = endpoint().trim().to_owned();
                                let result = request(connect(text)).await;
                                match result {
                                    Ok(client) => connection.set(Some(Connection(Arc::new(client)))),
                                    Err(problem) => error.set(Some(problem)),
                                }
                                connecting.set(false);
                            });
                        }, if connecting() { "Connecting…" } else { "Connect" } }
                        if let Some(problem) = error() { Notice { error: true, "{problem}" } }
                        p { class: "small muted", "Read-only telemetry. Treat your endpoint as private: anyone who has it can attempt to read the data you expose." }
                    }
                }
            }
            footer { class: "footer", "Rust SDK · Iroh transport · read-only game telemetry" }
        }
    }
}

#[component]
fn Connected(connection: Connection, on_disconnect: EventHandler<()>) -> Element {
    use_context_provider(|| connection.clone());
    let status = use_signal(|| None::<types::ServiceStatus>);
    let mut selected = use_signal(String::new);
    let mounted = use_signal(|| [true; 4]);
    let selection = status.read().as_ref().and_then(|value| {
        value
            .targets
            .iter()
            .find_map(|target| match &target.activity {
                types::TargetActivity::Observing { session_id, .. }
                    if *session_id == selected() =>
                {
                    connection
                        .0
                        .warframe()
                        .session(types::SessionInfo {
                            session: types::SessionRef {
                                run_id: value.cursor.run_id.clone(),
                                session_id: session_id.clone(),
                            },
                            provider_id: target.provider_id.clone(),
                            game_id: target.game_id.clone(),
                            target: target.target.clone(),
                            game_build: match &target.activity {
                                types::TargetActivity::Observing { game_build, .. } => {
                                    game_build.clone()
                                }
                                _ => None,
                            },
                        })
                        .ok()
                        .map(Session)
                }
                _ => None,
            })
    });
    // Key components by the captured service run and session identity.
    let session_key = format!(
        "{}:{}",
        status
            .read()
            .as_ref()
            .map_or("", |s| s.cursor.run_id.as_str()),
        selected()
    );
    rsx! {
        main { class: "dashboard",
            Service { status, on_disconnect }
            div { class: "section-heading",
                div { p { class: "eyebrow", "LIVE WORKSPACE" } h2 { "Topic panels" } }
                div { class: "session-picker",
                    label { r#for: "session", "Session" }
                    select { id: "session", value: selected(), onchange: move |event| selected.set(event.value()),
                        option { value: "", "Select a session" }
                        if let Some(current) = status() {
                            for target in current.targets {
                                if let types::TargetActivity::Observing { session_id, .. } = target.activity {
                                    option { value: session_id.clone(), "{target.target.executable} · {session_id}" }
                                }
                            }
                        }
                    }
                }
            }
            if selected().is_empty() {
                Notice { "Choose a game session to open its panels." }
            } else if let Some(reference) = selection {
                Panels { key: "{session_key}", session: reference, mounted }
            } else {
                Notice { "This session has ended. Select another session to continue." }
            }
        }
    }
}
