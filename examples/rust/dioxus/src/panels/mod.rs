mod chat;
mod currencies;
mod inventory;
mod mastery;
mod player;
mod relic_rewards;
mod screens;
mod service;

pub use service::Service;

use crate::sdk::Session;
use dioxus::prelude::*;

#[component]
pub fn Panels(session: Session, mut mounted: Signal<[bool; 7]>) -> Element {
    use_context_provider(|| session.clone());
    rsx! {
        div { class: "panel-controls",
            for (index, name) in ["Player", "Currencies", "Inventory", "Chat", "Screens", "Relic rewards", "Mastery"].into_iter().enumerate() {
                label { class: "toggle",
                    input { r#type: "checkbox", checked: mounted()[index], onchange: move |event| mounted.write()[index] = event.checked() }
                    "{name}"
                }
            }
            span { class: "small muted", "Unmount a panel to release its demand." }
        }
        div { class: "panel-grid",
            if mounted()[0] { player::Player {} }
            if mounted()[1] { currencies::Currencies {} }
            if mounted()[2] { inventory::Inventory {} }
            if mounted()[3] { chat::Chat {} }
            if mounted()[4] { screens::Screens {} }
            if mounted()[5] { relic_rewards::RelicRewards {} }
            if mounted()[6] { mastery::Mastery {} }
        }
    }
}
