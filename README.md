# Warframe Observer

> [!IMPORTANT]
> The published release does not contain the api yet, if you want to test it out early, youll have to build everything yourself from main.

A background service that reads Warframe memory on Linux and Windows.

We only read memory. We never write to it or inject code.

## API

| Topic | Data | Type |
| --- | --- | --- |
| `warframe.inventory` | Item keys and quantities, grouped by inventory family. | Snapshot |
| `warframe.currencies` | Credits, Endo, tradable and non-tradable Platinum. | Snapshot |
| `warframe.player` | Account ID and username. | Snapshot |
| `warframe.chat` | Channel, sender, message text and optional game-local hour/minute. | Event |
| `warframe.screens` | Visible interface screens, including unknown movie asset paths. | Snapshot |
| `warframe.relic_rewards` | Current relic picker: closed or up to four ordered choices. | Snapshot |

Snapshot topics support `read()` for one sample, `watch()` for updates, and
`cached()` to inspect the service cache. Chat is an event stream with explicit
gap notifications; it has no snapshot API.

```rust,no_run
# async fn example() -> Result<(), wf_observer_sdk::ObserverError> {
let client = wf_observer_sdk::connect_local().await?;
let game = client.warframe().single_session().await?;
let screens = game.screens().read().await?.screens;
let picker = game.relic_rewards().read().await?.picker;
let mut updates = game.relic_rewards().watch().await?.into_stream();
# client.shutdown().await?;
# Ok(()) }
```

The service defaults to local-only access (applications running on your machine).
If you want remote connections (ie an application on your phone, another device etc.)

```wf-observer access remote```

and approve your connection key with:

``` wf-observer peers allow <endpointID>```

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
