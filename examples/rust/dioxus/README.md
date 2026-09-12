# Dioxus showcase

Follow the [service setup](../../../README.md#run-from-source), then run from the
repository root:

```bash
cargo run --locked -p example-rust-dioxus --features dioxus/desktop
# With the Dioxus CLI installed, use the browser renderer:
dx serve -p example-rust-dioxus --web
```

Paste the endpoint ID or an Iroh ticket into the connection form. The catalog
and process health work without a running game; topic data requires a supported,
logged-in game. See the [endpoint access limitation](../../../README.md).

The showcase shows player identity, all four currency balances, searchable and
paged inventory, and live chat with channel filtering and game-local timestamps.
Choose one process. Each panel can be independently unmounted to
release its subscription. Mount it again to resume after a terminal error.
The chat view retains its latest 200 events and marks source-continuity gaps.

[sdk.rs](src/sdk.rs) connects SDK streams to component signals. Each mounted
panel owns a watch; unmounting drops it. The currencies panel also demonstrates
`cached()`. Disconnect shuts down the client and unmounts all panels.

Native Button, Input and Card components are adapted from the Dioxus components
repository; see [THIRD_PARTY.md](THIRD_PARTY.md) for attribution and its license.
