# Examples

The [Dioxus showcase](rust/dioxus) displays player details, currencies,
inventory, chat, and service status through typed SDK watches.

Each console example connects, selects the single Warframe session, reads its
currency balances, and shuts down the client using a generated binding.

Run these commands from the repository root, log in to Warframe, and copy the
endpoint ID from `status`:

```bash
cargo run --locked -p wf-observer-cli -- start
cargo run --locked -p wf-observer-cli -- status
just example python csharp java kotlin --endpoint ENDPOINT_ID
# macOS only:
just example swift --endpoint ENDPOINT_ID
```

Install the [binding prerequisites](../CI.md#generated-bindings) for the selected
languages. The runner packages each binding once; `--no-package` reuses packages
under `dist/`. It leaves the service running when the examples finish.

To build compiled examples and import the Python example without a service or
game:

```bash
just example python csharp java kotlin --check
```

The examples require exactly one observing Warframe session. With several
sessions, use `client.warframe().sessions()` and `session(info)` to select one.
Reads have a default 30-second deadline and return errors for unavailable data.

Await watch and client shutdown before disposing generated objects; JVM
`close()` cannot await network cleanup. Kotlin/JVM uses Java's
`CompletableFuture`; coroutine applications can use an `await()` adapter.

API behavior and Rust usage examples live in the
[SDK documentation](../crates/wf-observer-sdk/src/lib.rs). Endpoint access
limitations are described in the [project overview](../README.md).
