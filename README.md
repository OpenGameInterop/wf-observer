# Warframe Observer

> [!IMPORTANT]
> This repository currently does none of the advertised features. The memory reading api has been proven in a private repo and is currently undergoing cleanup before being integrated into this one.

A local background process that reads memory from a running Warframe process on
Linux and Windows and makes the data available to other applications.

We only read memory. We never write to it or inject code.

You must explicitly run `wf-observer attach` to start it. This requires
Warframe to be open. The process shuts itself down when Warframe closes.

Use `wf-observer status` to inspect the running process, its Warframe process,
version, and Iroh endpoint identifier.
Use `wf-observer stop` to request a shutdown without closing Warframe.

## Installation

### Windows (Scoop)

```powershell
scoop bucket add opengameinterop https://github.com/OpenGameInterop/scoop-bucket
scoop install opengameinterop/wf-observer
```

Stop an attached agent before updating if neccessary because Scoop will not replace a
running executable:

```powershell
wf-observer stop
scoop update wf-observer
```

### Arch Linux

The prepared AUR packages are `wf-observer`, which builds from source, and
`wf-observer-bin`, which installs the release binary. They are not currently
published because new AUR account registration is unavailable and I do not yet have an account, sorry x)

Until AUR publication is available, download the Linux archive from the latest
`cli-v*` entry on the
[GitHub releases page](https://github.com/OpenGameInterop/wf-observer/releases), then
install its executable:

```bash
mkdir wf-observer-release
tar -xzf wf-observer-*-x86_64-unknown-linux-gnu.tar.gz -C wf-observer-release
sudo install -Dm755 wf-observer-release/wf-observer /usr/local/bin/wf-observer
```
An alternative to AUR exists in a third-party Arch repository [here](https://git.denaerium.com/Denaerium/-/packages/arch/wf-observer-bin/). 
Instructions on how to add the repository and use it can be found [here](https://git.denaerium.com/Denaerium/-/packages/arch/wf-observer-bin/).
This external repository makes no guarantees on availability, but should be
an acceptable mostly-always up alternative if AUR temporarily experiences outages.

Additionally, the external repository also hosts a `wf-observer-git` package which
updates on each commit made to `main`. It is available [here](https://git.denaerium.com/Denaerium/-/packages/arch/wf-observer-git/).
It updates on a timer, and a check is made once every 30 minutes.

## Running the example integration

```bash
# be at the root of the repo
# start Warframe, then attach the background agent
cargo run -p wf-observer-cli -- attach
# run the showcase
cargo run -p example-rust-dioxus --features dioxus/desktop
```

Foreign-language clients are generated from `wf_observer_ffi` by
[BoltFFI](https://www.boltffi.dev/). Swift, Java, C#, Python, and browser
TypeScript are configured in `boltffi.toml`. Android bindings are currently
unsupported and disabled.

See the [examples](examples/README.md) for the Dioxus showcase and
minimal generated-binding consumers.

## Profiling

[Hotpath](https://hotpath.rs) profiling is opt-in.
CPU sampling is unavailable on Windows; omit `hotpath/hotpath-cpu` there.

```bash
cargo run -p wf-observer-cli --features hotpath/hotpath,hotpath/hotpath-alloc,hotpath/hotpath-cpu -- attach
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you shall be dual licensed as above, without
any additional terms or conditions.
