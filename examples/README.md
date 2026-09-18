# Examples

The [Dioxus showcase](rust/dioxus) displays player details, currencies,
inventory, mastery, chat, screens, relic rewards, and service status through typed SDK watches.

Each console example connects, selects the single Warframe session, reads its
currency balances, and shuts down the client using a generated binding.

Run these commands from the repository root, log in to Warframe, and copy the
local connection ticket from `status`:

```bash
cargo run --locked -p wf-observer-cli -- start
cargo run --locked -p wf-observer-cli -- status
just example python csharp java kotlin --endpoint LOCAL_TICKET
# macOS only:
just example swift --endpoint LOCAL_TICKET
```

Install the [binding prerequisites](../CI.md#generated-bindings) for the selected
languages. The runner packages each binding once; `--no-package` reuses packages
under `dist/`. It leaves the service running when the examples finish.

Each example prints its reader ID before connecting and keeps its key under
`~/.wf-observer-examples/<language>.key` in the user's home directory. For remote
access, follow the [approval workflow](../README.md) using that reader ID, then
rerun the example with the service endpoint ID.

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
