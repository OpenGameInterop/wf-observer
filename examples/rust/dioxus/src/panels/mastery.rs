use crate::{
    components::{Button, Card, Input, Notice},
    sdk::{Session, use_watch},
};
use dioxus::prelude::*;
use wf_observer_sdk::MasteryState;

const PAGE_SIZE: usize = 100;

#[component]
pub fn Mastery() -> Element {
    let Session(session) = use_context();
    let live = use_watch(
        move || {
            let session = session.clone();
            async move { Ok(session.mastery().watch().await?.into_stream()) }
        },
        None,
    );
    let mut query = use_signal(String::new);
    let mut page = use_signal(|| 0_usize);
    let current = (live.value)();
    let mastery = match &current {
        Some(MasteryState::Ready { value }) => Some(value),
        _ => None,
    };
    let filter = query().to_lowercase();
    let rows: Vec<_> = mastery
        .into_iter()
        .flat_map(|value| &value.items)
        .filter(|item| item.item_key.to_lowercase().contains(&filter))
        .collect();
    let total_pages = rows.len().div_ceil(PAGE_SIZE).max(1);
    let current_page = page().min(total_pages - 1);
    rsx! {
        div { class: "wide-panel",
            Card { title: "Mastery", subtitle: "Completed rank, mastery points, and retained item affinity.",
                if let Some(problem) = (live.error)() { Notice { error: true, "{problem}" } }
                match &current {
                    Some(MasteryState::Unavailable { reason }) => rsx! { Notice { "Unavailable · {reason}" } },
                    Some(MasteryState::Ready { .. }) => rsx! {},
                    _ => rsx! { Notice { "Waiting for mastery…" } },
                }
                if let Some(value) = mastery {
                    div { class: "balance-grid",
                        for (name, amount) in [
                            ("Completed rank", u64::from(value.rank)),
                            ("Total mastery points", value.total_points),
                            ("Item mastery points", value.item_points),
                            ("Tracked items", value.items.len() as u64),
                        ] {
                            div { class: "balance", span { "{name}" } strong { "{amount}" } }
                        }
                    }
                    details { class: "inspector", summary { "Account identity" } code { "{value.account_id}" } }
                    p { class: "small muted", "Retained affinity can exceed an item's maximum-rank threshold and includes equipment you no longer own." }
                    div { class: "filters",
                        div {
                            label { r#for: "mastery-search", "Search item path" }
                            Input { id: "mastery-search", value: query(), placeholder: "Filter retained progression", oninput: move |event: FormEvent| { query.set(event.value()); page.set(0); } }
                        }
                    }
                    div { class: "table-scroll",
                        table {
                            caption { "{rows.len()} matching items · page {current_page + 1} of {total_pages}" }
                            thead { tr { th { "Item path" } th { class: "numeric", "Retained affinity" } } }
                            tbody {
                                for item in rows.iter().skip(current_page * PAGE_SIZE).take(PAGE_SIZE) {
                                    tr { key: "{item.item_key}",
                                        td { code { "{item.item_key}" } }
                                        td { class: "numeric", "{item.affinity}" }
                                    }
                                }
                            }
                        }
                    }
                    if rows.is_empty() { Notice { "No retained items match this selection." } }
                    div { class: "actions pagination",
                        Button { secondary: true, disabled: current_page == 0, onclick: move |_| page.set(current_page.saturating_sub(1)), "Previous" }
                        Button { secondary: true, disabled: current_page + 1 >= total_pages, onclick: move |_| page.set(current_page + 1), "Next" }
                    }
                }
            }
        }
    }
}
