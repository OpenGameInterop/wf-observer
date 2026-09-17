use dioxus::prelude::*;
use std::sync::Arc;
use wf_observer_sdk::{self as types};

use crate::{
    components::{Button, Card, Input, Notice},
    panels::{Panels, Service},
    sdk::{Connection, Session, request},
};

const STYLES: &str = include_str!("../assets/app.css");

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
const CONNECTION_HELP: &str = "Start the service, then copy the local connection ticket from wf-observer status. For another device, enable remote access and use its endpoint ID.";
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
const CONNECTION_HELP: &str = "Enable wf-observer access remote, then copy the endpoint ID from wf-observer status. Browser connections require remote access.";

#[component]
pub fn App() -> Element {
    let identity = use_signal(crate::identity::load);
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
                        p { "Explore live player data, balances, inventory, chat, screens and relic rewards through the Rust SDK." }
                        div { class: "topic-chips", for name in ["Player", "Currencies", "Inventory", "Chat", "Screens", "Relic rewards"] { span { "{name}" } } }
                    }
                    Card { title: "Connect to your observer", subtitle: CONNECTION_HELP,
                        if let Ok(reader) = identity() {
                            label { r#for: "reader-id", "This app's reader ID" }
                            input { class: "input", id: "reader-id", readonly: true, value: reader.endpoint_id() }
                            p { class: "small muted", "For remote access, approve this ID on the service device, then connect:" }
                            code { class: "approval-command", "wf-observer peers allow {reader.endpoint_id()}" }
                        } else if let Err(problem) = identity() {
                            Notice { error: true, "{problem}" }
                        }
                        label { r#for: "endpoint", "Endpoint ID or ticket" }
                        Input { id: "endpoint", value: endpoint(), placeholder: "Paste your endpoint ID or ticket", oninput: move |event: FormEvent| endpoint.set(event.value()) }
                        Button { disabled: connecting() || endpoint().trim().is_empty() || identity().is_err(), onclick: move |_| {
                            connecting.set(true);
                            error.set(None);
                            spawn(async move {
                                let text = endpoint().trim().to_owned();
                                let result = match identity() {
                                    Ok(reader) => request(reader.connect(text)).await,
                                    Err(problem) => Err(problem),
                                };
                                match result {
                                    Ok(client) => connection.set(Some(Connection(Arc::new(client)))),
                                    Err(problem) => error.set(Some(problem)),
                                }
                                connecting.set(false);
                            });
                        }, if connecting() { "Connecting…" } else { "Connect" } }
                        if let Some(problem) = error() { Notice { error: true, "{problem}" } }
                        p { class: "small muted", "This app keeps its reader identity for future connections. Remote access requires approval; local-only access works without it." }
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
    let mounted = use_signal(|| [true; 6]);
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
