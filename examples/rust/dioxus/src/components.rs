//! Native controls adapted from DioxusLabs/dioxus-components; see `THIRD_PARTY.md`.

use dioxus::prelude::*;

#[component]
pub fn Button(
    #[props(default)] secondary: bool,
    #[props(default)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "button",
            "data-style": if secondary { "secondary" } else { "primary" },
            disabled,
            onclick,
            {children}
        }
    }
}

#[component]
pub fn Input(
    id: String,
    value: String,
    placeholder: String,
    oninput: EventHandler<FormEvent>,
) -> Element {
    rsx! { input { class: "input", id, value, placeholder, oninput } }
}

#[component]
pub fn Card(title: String, subtitle: String, children: Element) -> Element {
    rsx! {
        section { class: "card", "data-slot": "card",
            header { class: "card-header", "data-slot": "card-header",
                h2 { "{title}" }
                p { "{subtitle}" }
            }
            div { class: "card-content", "data-slot": "card-content", {children} }
        }
    }
}

#[component]
pub fn Notice(#[props(default)] error: bool, children: Element) -> Element {
    rsx! {
        p { class: if error { "notice error" } else { "notice" }, role: "status", {children} }
    }
}
