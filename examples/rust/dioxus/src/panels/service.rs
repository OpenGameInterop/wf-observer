use dioxus::prelude::*;
use std::time::Duration;
use wf_observer_sdk as types;

use crate::{
    components::{Button, Notice},
    sdk::{Connection, health, request},
};

#[component]
pub fn Service(
    mut status: Signal<Option<types::ServiceStatus>>,
    on_disconnect: EventHandler<()>,
) -> Element {
    let Connection(client) = use_context();
    let mut error = use_signal(|| None::<String>);
    let mut ping = use_signal(|| None::<String>);
    let catalog_client = client.clone();
    let catalog = use_resource(move || {
        let client = catalog_client.clone();
        async move { request(client.catalog()).await }
    });
    let status_client = client.clone();
    use_future(move || {
        let client = status_client.clone();
        async move {
            loop {
                match request(client.status()).await {
                    Ok(current) => {
                        status.set(Some(current));
                        error.set(None);
                    }
                    Err(problem) => {
                        error.set(Some(problem));
                    }
                }
                n0_future::time::sleep(Duration::from_secs(2)).await;
            }
        }
    });
    rsx! {
        section { class: "service-bar",
            div {
                p { class: "eyebrow", "OBSERVER SERVICE" }
                if let Some(current) = status() {
                    h2 { "{current.targets.len()} game processes" }
                    p { class: "small muted", "v{current.application_version} · {current.discovery:?}" }
                } else { h2 { "Connecting to service state…" } }
            }
            div { class: "actions",
                Button { secondary: true, onclick: move |_| {
                    let client = client.clone();
                    ping.set(Some("Pinging…".into()));
                    spawn(async move {
                        let start = n0_future::time::Instant::now();
                        ping.set(Some(match request(client.ping()).await {
                            Ok(()) => format!("Pong · {} ms", start.elapsed().as_millis()),
                            Err(problem) => problem,
                        }));
                    });
                }, "Ping" }
                Button { secondary: true, onclick: move |_| on_disconnect.call(()), "Disconnect" }
            }
            if let Some(value) = ping() { p { class: "small", role: "status", "{value}" } }
            if let Some(problem) = error() { Notice { error: true, "Status refresh failed; showing last known process metadata. {problem}" } }
        }
        details { class: "service-details",
            summary { "Catalog & process health" }
            div { class: "diagnostic-grid",
                div {
                    h3 { "Compiled capabilities" }
                    match &*catalog.read_unchecked() {
                        Some(Ok(value)) => rsx! { for provider in &value.providers {
                            h4 { "{provider.name} / {provider.version}" }
                            ul { for cap in &provider.capabilities {
                                li { code { "{cap.topic}" } " · schema {cap.schema_version} · snapshots: {cap.snapshots} · events: {cap.events}" }
                            } }
                        } },
                        Some(Err(problem)) => rsx! { Notice { error: true, "{problem}" } },
                        None => rsx! { p { "Loading catalog…" } },
                    }
                }
                div {
                    h3 { "Discovered processes" }
                    if let Some(current) = status() {
                        for target in current.targets {
                            h4 { "{target.target.executable} · PID {target.target.pid}" }
                            match target.activity {
                                types::TargetActivity::Attaching => rsx! { p { "Attaching…" } },
                                types::TargetActivity::Retrying { message } => rsx! { Notice { error: true, "{message}" } },
                                types::TargetActivity::Observing { session_id, game_build, topics } => rsx! {
                                    p { class: "small", "Session {session_id} · build " {game_build.as_deref().unwrap_or("unresolved")} }
                                    ul { for topic in topics { li { "{topic.topic.topic} · {health(&topic.health)}" } } }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}
