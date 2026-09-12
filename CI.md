# Continuous integration

This document describes the CI workflows. Where applicable, each section ends
with the equivalent command for running the check locally.

Pull requests always create one stable `CI / required` check. A lightweight
planning job classifies the changed files, runs formatting and workflow linting
when relevant, and starts only the affected component jobs. The final check
fails unless every selected component succeeds and every unselected component
is skipped. Dependency groups are documented as data in
`.github/ci-paths.yml`; changing that policy intentionally exercises every CI
component.

## Core Rust Checks

Linux runs formatting, Clippy, documentation, and tests for the core workspace.
Windows runs the platform-specific core tests without repeating the
platform-independent lint and documentation work.

Rustdoc treats broken intra-doc links as errors.

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --exclude example-rust-dioxus --all-targets --all-features -- -D warnings
cargo doc --locked --workspace --exclude example-rust-dioxus --no-deps --all-features
cargo test --locked --workspace --exclude example-rust-dioxus --all-features
cargo clippy --locked -p example-rust-dioxus --all-targets --features dioxus/desktop -- -D warnings
cargo build --locked -p example-rust-dioxus --features dioxus/desktop
```

`just check` runs this local sequence. With the browser target installed, also run:

```bash
cargo check --locked --target wasm32-unknown-unknown -p example-rust-dioxus --features dioxus/web
```

## Workflow Linting

Actionlint runs inside the planning job whenever a workflow changes. A change
to the required CI workflow selects every component so modifications to CI
steps are exercised, not merely parsed.

Requires [Actionlint](https://github.com/rhysd/actionlint) 1.7.12 locally.

```bash
actionlint
```

## Language Bindings

The required CI workflow packages selected bindings on Linux. Java,
Gradle, .NET, Python, Node.js/TypeScript, and their package/example steps are enabled
independently from the changed paths. A Gradle-only change therefore runs only
the JVM portion. Shared SDK, portable model, or binding configuration
changes select every affected language; native provider changes use core Rust CI. Swift
runs on macOS only for shared or Swift-specific changes. Artifact uploads are
reserved for release workflows.

`boltffi.ci.toml` limits that pull-request job to its macOS ARM64 slice; release
packaging continues to build the complete Apple matrix from `boltffi.toml`.

Generated files live under the ignored `dist/` directory.

Requires [Just](https://just.systems/), [BoltFFI](https://www.boltffi.dev/)
0.30.1, Clang, JDK 17, the .NET 10 SDK, and Python 3.10 or newer. Browser
packaging also requires Node.js and TypeScript (`tsc` on PATH); CI uses Node 24
and TypeScript 5.9.3. Optimized browser releases need Binaryen 123 or newer. Swift
packaging additionally requires macOS and Xcode. `pack` regenerates the binding
before building the artifact consumed by each example.

```bash
cargo install --locked --version 0.30.1 boltffi_cli
rustup target add wasm32-unknown-unknown
just binding csharp
just binding java
just binding python --python python
just binding wasm
# macOS only
just binding apple
```

The example runner packages each required binding once and checks the consumers
without starting a service or requiring a game:

```bash
just example python csharp java kotlin --check
# macOS only
just example swift --check
```

Compiled examples are built against the generated packages. Python imports the
wheel and example without executing its entrypoint. SDK and native-runtime behavior is
covered by the ordinary Rust integration tests in the core workspace.

To exercise a generated client against a logged-in game locally, start the
normal service and pass its endpoint ID to the same example:

```bash
wf-observer start
wf-observer status
just example python --endpoint ENDPOINT_ID
```

This prints currency balances and shuts down the client; it does not stop the
service.

The Dioxus showcase is linted and linked on Linux, checked for the browser target
on Linux, and checked for desktop on Windows when the showcase or a dependency
changes. Game-dependent behavior is not tested in CI.

Android generation remains disabled until the generated JNI boundary installs
the JVM context required by [Iroh on Android](https://docs.rs/iroh/latest/iroh/endpoint/struct.Endpoint.html#usage-on-android).

## Release bindings

The Release bindings workflow runs for version tags and on manual request. It
builds JVM bundles and Python wheels on every native host supported by BoltFFI,
assembles one six-RID NuGet package, and packages the Apple and browser targets.
Android remains excluded until its Iroh initialization is implemented.

Python wheels cover every supported host for CPython 3.10 through 3.14. The
workflow uploads artifacts to its Actions run; publishing them to package
registries remains a separate release step. Final binding bundles are retained
for seven days; intermediate C# native libraries are retained for one day.

JVM artifacts are target-specific because BoltFFI 0.30.1 cannot combine
cross-host JVM builds. The NuGet assembly job uses `boltffi.release.toml` to
combine native libraries built by the platform matrix.

The equivalent command for one local desktop target is:

```bash
just binding java --release
just binding python --release --python python
just binding csharp --release
```

Apple and browser packages can be built on their respective hosts with:

```bash
just binding apple --release
just binding wasm --release
```

## Release CLI

The CLI has an independent package version in
`crates/wf-observer-cli/Cargo.toml`. The Release CLI workflow builds Linux and
Windows x64 archives on manual runs without publishing them. Pushing a
`cli-v{version}` tag additionally creates a GitHub Release containing both
binary archives and `SHA256SUMS`. Stable versions also produce rendered AUR
and Scoop metadata; tagged releases publish that metadata to the configured
package repositories.

The release validator rejects a tag which does not exactly match the CLI
package version or is not newer than every existing `cli-v*` tag. Check the
current version locally, or dry-run a prospective tag, with:

```bash
cargo run --locked -p xtask -- release check-cli
version="$(cargo run --quiet --locked -p xtask -- release check-cli)"
cargo run --locked -p xtask -- release check-cli --tag "cli-v$version"
```

See [RELEASING.md](RELEASING.md) for the complete release sequence. Binary AUR
and Scoop publication consume these archives, while a source-based AUR package
builds from GitHub's tag archive. Package publication is maintained separately
from the artifact workflow.

## Link checking

Rust files are scanned as plain text, which catches full URLs in doc
comments and string literals. A scheduled weekly workflow scans the entire
repository so external link rot is still detected without blocking unrelated
pull requests.

Requires [Lychee](https://github.com/lycheeverse/lychee) to be installed
locally.

```bash
lychee './**/*.md' './**/*.rs' './**/*.toml' './**/*.yml' './**/*.yaml' './**/*.css'
```

## Caches

Pull-request jobs restore caches but do not save multi-gigabyte target caches
under pull-request-only refs. After dependency manifests, the lockfile, or the
toolchain change on `main`, the `Warm CI caches` workflow refreshes the shared
Linux and Windows dependency caches and the smaller Linux/macOS BoltFFI tool
caches.

## SemVer checking

Cargo SemVer Checks compares the public API against the latest published
version on crates.io. Its pull request trigger is disabled because none of the
workspace library crates has a published crates.io baseline. CLI GitHub
releases do not provide that baseline.

Requires [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks)
to be installed locally.

```bash
cargo semver-checks
```