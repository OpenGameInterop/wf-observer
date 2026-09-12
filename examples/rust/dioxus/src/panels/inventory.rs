use crate::{
    components::{Button, Card, Input, Notice},
    sdk::{Session, use_watch},
};
use dioxus::prelude::*;
use wf_observer_sdk::{InventoryFamily, InventoryState};

const PAGE_SIZE: usize = 100;

#[component]
pub fn Inventory() -> Element {
    let Session(session) = use_context();
    let live = use_watch(
        move || {
            let session = session.clone();
            async move { Ok(session.inventory().watch().await?.into_stream()) }
        },
        None,
    );
    let mut query = use_signal(String::new);
    let mut family = use_signal(String::new);
    let mut page = use_signal(|| 0_usize);
    let current = (live.value)();
    let inventory = match &current {
        Some(InventoryState::Ready { value }) => Some(value),
        _ => None,
    };
    let filter = query().to_lowercase();
    let selected_family = family();
    let mut rows = Vec::new();
    if let Some(value) = inventory {
        for entry in &value.families {
            if !selected_family.is_empty() && format!("{:?}", entry.family) != selected_family {
                continue;
            }
            for item in &entry.items {
                if item.item_key.to_lowercase().contains(&filter) {
                    rows.push((&value.metadata.source.session, entry.family, item));
                }
            }
        }
    }
    let total_pages = rows.len().div_ceil(PAGE_SIZE).max(1);
    let current_page = page().min(total_pages - 1);
    rsx! {
        div { class: "wide-panel",
            Card { title: "Inventory", subtitle: "Canonical item paths and exact quantities. Empty and unavailable are distinct.",
                if let Some(problem) = (live.error)() { Notice { error: true, "{problem}" } }
                match &current {
                    Some(InventoryState::Unavailable { reason }) => rsx! { Notice { "Unavailable · {reason}" } },
                    Some(InventoryState::Ready { .. }) => rsx! {},
                    _ => rsx! { Notice { "Waiting for inventory…" } },
                }
                div { class: "filters",
                    div { label { r#for: "item-search", "Search item path" }
                        Input { id: "item-search", value: query(), placeholder: "e.g. AlloyPlate", oninput: move |event: FormEvent| { query.set(event.value()); page.set(0); } }
                    }
                    div { label { r#for: "family", "Family" }
                        select { id: "family", value: family(), onchange: move |event| { family.set(event.value()); page.set(0); },
                            option { value: "", "All families" }
                            for entry in InventoryFamily::ALL { option { value: format!("{entry:?}"), "{entry:?}" } }
                        }
                    }
                }
                if inventory.is_some() {
                    div { class: "table-scroll",
                        table {
                            caption { "{rows.len()} matching records · page {current_page + 1} of {total_pages}" }
                            thead { tr { th { "Item path" } th { "Family" } th { "Session" } th { class: "numeric", "Owned" } } }
                            tbody {
                                for (session, family, item) in rows.iter().skip(current_page * PAGE_SIZE).take(PAGE_SIZE) {
                                    tr { key: "{session.session_id}:{family:?}:{item.item_key}",
                                        td { code { "{item.item_key}" } } td { "{family:?}" } td { class: "small", "{session.session_id}" } td { class: "numeric", "{item.quantity}" }
                                    }
                                }
                            }
                        }
                    }
                    if rows.is_empty() { Notice { "No items match this selection." } }
                    div { class: "actions pagination",
                        Button { secondary: true, disabled: current_page == 0, onclick: move |_| page.set(current_page.saturating_sub(1)), "Previous" }
                        Button { secondary: true, disabled: current_page + 1 >= total_pages, onclick: move |_| page.set(current_page + 1), "Next" }
                    }
                }
            }
        }
    }
}
