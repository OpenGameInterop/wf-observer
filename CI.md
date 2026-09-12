# Development checks

Pull requests use [`.github/ci-paths.yml`](.github/ci-paths.yml) to select jobs in
[CI](.github/workflows/ci.yml). The `CI / required` check passes when all selected
jobs succeed and unselected jobs are skipped. Changes to the workflow or path
policy select every component.

## Rust and Dioxus

Install [Just](https://just.systems/), then run:

```bash
just check
```

The [recipe](justfile) runs formatting, Clippy, Rustdoc, and core tests, then
lints and builds the Dioxus desktop example. CI runs core tests on Linux and
Windows; lint and documentation checks run on Linux.

For the browser target:

```bash
rustup target add wasm32-unknown-unknown
cargo check --locked --target wasm32-unknown-unknown -p example-rust-dioxus --features dioxus/web
```

CI checks Dioxus for desktop on Linux and Windows and for the browser on Linux.
It does not run Warframe. Native polling tests use synthetic memory and assume
successful layout validation; they cover acquisition and state changes.
Transport tests use synthetic provider publications. These tests do not establish
compatibility with a game executable.

## Generated bindings

Install [BoltFFI](https://www.boltffi.dev/) and the toolchain for each target:

```bash
cargo install --locked --version 0.30.1 boltffi_cli
```

| Target | Additional tools |
| --- | --- |
| Java / Kotlin | Clang and JDK 17; the example runner uses the Gradle wrapper |
| C# | Clang and .NET 10 SDK |
| Python | Clang and Python 3.10 or newer |
| Swift | macOS and Xcode |
| Browser | Node.js, TypeScript (`tsc`), and the Rust WASM target; optimized releases also need Binaryen 123 or newer |

Package a target with `just binding java`, `csharp`, `python`, `apple`, or `wasm`.
Generated files go under the ignored `dist/` directory. Check the console
examples without starting a service:

```bash
just example python csharp java kotlin --check
# macOS only:
just example swift --check
```

This builds compiled examples and imports the Python example. It does not test
generated clients against a running service. See the [examples](examples/README.md)
for that manual check. CI selects language jobs by changed paths; Swift runs on
macOS with the ARM64 overlay in [boltffi.ci.toml](boltffi.ci.toml).

## Workflows and links

Install [Actionlint](https://github.com/rhysd/actionlint) and
[Lychee](https://github.com/lycheeverse/lychee), then run:

```bash
actionlint
just links
```

CI lints changed workflows and checks links in changed files. A weekly workflow
checks links across the repository. The workflow files pin the CI tool versions.

## Profiling

[Hotpath](https://hotpath.rs) profiling is opt-in:

```bash
cargo run --locked -p wf-observer-cli --features hotpath/hotpath,hotpath/hotpath-alloc,hotpath/hotpath-cpu -- start
```

Omit `hotpath/hotpath-cpu` on Windows, where CPU sampling is unavailable.

## Maintenance workflows

Pull-request jobs restore shared caches. Cache-warming workflows refresh them
from `main`; their triggers and paths are defined in `.github/workflows/`.
Cargo SemVer Checks is disabled for pull requests until there is a published
crates.io version to compare against.

Release validation, artifact packaging, and publication are documented in
[RELEASING.md](RELEASING.md).
