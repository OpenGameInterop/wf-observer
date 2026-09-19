use crate::{
    components::{Button, Card, Input, Notice},
    sdk::{Session, use_watch},
};
use dioxus::prelude::*;
use wf_observer_sdk::StarChartState;

const PAGE_SIZE: usize = 100;

#[component]
pub fn StarChart() -> Element {
    let Session(session) = use_context();
    let live = use_watch(
        move || {
            let session = session.clone();
            async move { Ok(session.star_chart().watch().await?.into_stream()) }
        },
        None,
    );
    let mut query = use_signal(String::new);
    let mut page = use_signal(|| 0_usize);
    let current = (live.value)();
    let star_chart = match &current {
        Some(StarChartState::Ready { value }) => Some(value),
        _ => None,
    };
    let filter = query().to_lowercase();
    let rows: Vec<_> = star_chart
        .into_iter()
        .flat_map(|value| &value.nodes)
        .filter(|item| item.node_key.to_lowercase().contains(&filter))
        .collect();
    let total_pages = rows.len().div_ceil(PAGE_SIZE).max(1);
    let current_page = page().min(total_pages - 1);
    rsx! {
        div { class: "wide-panel",
            Card { title: "Star Chart", subtitle: "Retained node completion for Normal and Steel Path.",
                if let Some(problem) = (live.error)() { Notice { error: true, "{problem}" } }
                match &current {
                    Some(StarChartState::Unavailable { reason }) => rsx! { Notice { "Unavailable · {reason}" } },
                    Some(StarChartState::Ready { .. }) => rsx! {},
                    _ => rsx! { Notice { "Waiting for Star Chart…" } },
                }
                if star_chart.is_some() {
                    p { class: "small muted", "Retained records grant Normal completion credit. Steel Path is recorded separately; counts include all difficulties. Missing nodes do not imply they are locked." }
                    div { class: "filters",
                        div {
                            label { r#for: "star_chart-search", "Search node tag" }
                            Input { id: "star_chart-search", value: query(), placeholder: "Filter retained progression", oninput: move |event: FormEvent| { query.set(event.value()); page.set(0); } }
                        }
                    }
                    div { class: "table-scroll",
                        table {
                            caption { "{rows.len()} matching nodes · page {current_page + 1} of {total_pages}" }
                            thead { tr { th { "Node tag" } th { class: "numeric", "Completions" } th { "Normal" } th { "Steel Path" } } }
                            tbody {
                                for item in rows.iter().skip(current_page * PAGE_SIZE).take(PAGE_SIZE) {
                                    tr { key: "{item.node_key}",
                                        td { code { "{item.node_key}" } }
                                        td { class: "numeric", "{item.completions}" }
                                        td { "Completed" }
                                        td { if item.steel_path_completed { "Completed" } else { "No record" } }
                                    }
                                }
                            }
                        }
                    }
                    if rows.is_empty() { Notice { "No retained nodes match this selection." } }
                    div { class: "actions pagination",
                        Button { secondary: true, disabled: current_page == 0, onclick: move |_| page.set(current_page.saturating_sub(1)), "Previous" }
                        Button { secondary: true, disabled: current_page + 1 >= total_pages, onclick: move |_| page.set(current_page + 1), "Next" }
                    }
                }
            }
        }
    }
}
