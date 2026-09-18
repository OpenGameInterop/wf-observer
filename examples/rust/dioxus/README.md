# Dioxus showcase

Follow the [service setup](../../../README.md), then run from the
repository root:

```bash
cargo run --locked -p example-rust-dioxus --features dioxus/desktop
# With the Dioxus CLI installed, use the browser renderer:
dx serve -p example-rust-dioxus --web
```

On the service machine, the desktop renderer accepts the local ticket from
`wf-observer status`. Browsers and other devices require remote access and the
service endpoint ID. The form displays the reader ID and approval command from
the [service setup](../../../README.md).

Desktop keys live in the `wf-observer-showcase` local configuration directory;
browser keys use local storage for the showcase's origin. Clearing storage or
changing browser profiles requires a new approval.

Catalog and process health work without a game. Screen detection requires a
supported running game; account topics additionally require a logged-in account.

The showcase shows player identity, all four currency balances, searchable and
paged inventory, live chat with channel filtering and game-local timestamps,
visible screens, and the current relic picker with ordered StoreItem keys.
The mastery panel shows completed rank, total/item mastery points, and searchable,
paged retained affinity by canonical item path.
Choose one process. Each panel can be independently unmounted to
release its subscription. Mount it again to resume after a terminal error.
The chat view retains its latest 200 events and marks source-continuity gaps.

[sdk.rs](src/sdk.rs) connects SDK streams to component signals. Each mounted
panel owns a watch; unmounting drops it. The currencies panel also demonstrates
`cached()`. Disconnect shuts down the client and unmounts all panels.

Native Button, Input and Card components are adapted from the Dioxus components
repository; see [THIRD_PARTY.md](THIRD_PARTY.md) for attribution and its license.
