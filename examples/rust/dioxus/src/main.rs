mod app;
mod components;
mod panels;
mod sdk;

#[hotpath::main]
fn main() {
    dioxus::launch(app::App);
}
