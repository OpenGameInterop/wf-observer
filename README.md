# Warframe Observer

A background service that reads Warframe memory on Linux and Windows and exposes
inventory, currencies, player identity, and chat through a shared SDK.

We only read memory. We never write to it or inject code.

The service accepts network connections through Iroh, including its default
relays. Reader access control is not implemented: anyone with the endpoint ID can
read the exposed data. Keep endpoint IDs and tickets private.

## Run from source

The topic API described here is unreleased. The published CLI 0.0.1 predates it;
use this checkout for the service and examples. From the repository root:

```bash
cargo run --locked -p wf-observer-cli -- start
cargo run --locked -p wf-observer-cli -- status
cargo run --locked -p example-rust-dioxus --features dioxus/desktop
```

Paste the endpoint ID from `status` into the showcase. Warframe can be started
before or after the service. Topic data requires a supported, logged-in game.
Use `cargo run --locked -p wf-observer-cli -- stop` to stop the service.

## API

| Topic | Data | Type |
| --- | --- | --- |
| `warframe.inventory` | Item keys and quantities, grouped by inventory family. | Snapshot |
| `warframe.currencies` | Credits, Endo, tradable and non-tradable Platinum. | Snapshot |
| `warframe.player` | Account ID and username. | Snapshot |
| `warframe.chat` | Channel, sender, message text and optional game-local hour/minute. | Event |

Snapshot topics support `read()` for one sample, `watch()` for updates, and
`cached()` to inspect the service cache. Chat is an event stream with explicit
gap notifications; it has no snapshot API.

The Rust package is `wf_observer_sdk`. [BoltFFI](https://www.boltffi.dev/)
generates Swift, Java, C#, Python, and browser TypeScript bindings from the same
crate. Kotlin/JVM uses the Java binding; Android packaging is disabled.
See the [examples](examples/README.md) for setup and usage.

## Released CLI

These packages install the released CLI, which has its own versioned commands.
Check `wf-observer --help` after installation.

### Windows (Scoop)

```powershell
scoop bucket add opengameinterop https://github.com/OpenGameInterop/scoop-bucket
scoop install opengameinterop/wf-observer
```

Stop the service before updating if necessary because Scoop will not replace a
running executable:

```powershell
wf-observer stop
scoop update wf-observer
```

### Arch Linux

Download the Linux archive from a `cli-v*` entry on the
[GitHub releases page](https://github.com/OpenGameInterop/wf-observer/releases), then
install its executable:

```bash
mkdir wf-observer-release
tar -xzf wf-observer-*-x86_64-unknown-linux-gnu.tar.gz -C wf-observer-release
sudo install -Dm755 wf-observer-release/wf-observer /usr/local/bin/wf-observer
```

Third-party Arch packages are available for the
[release binary](https://git.denaerium.com/Denaerium/-/packages/arch/wf-observer-bin/)
and [builds from `main`](https://git.denaerium.com/Denaerium/-/packages/arch/wf-observer-git/).
AUR publication is disabled in this repository's release workflow.

## Development

See [CI.md](CI.md) for local checks and profiling, and
[RELEASING.md](RELEASING.md) for release procedures.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you shall be dual licensed as above, without
any additional terms or conditions.
