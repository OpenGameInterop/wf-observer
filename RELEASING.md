# Releasing WF Observer

The CLI uses `cli-v{version}` tags. SDK bindings use `v{version}` tags and the
[Release bindings](.github/workflows/bindings-release.yml) workflow.

## CLI release

1. Update `version` in `crates/wf-observer-cli/Cargo.toml` and the corresponding
   `Cargo.lock` entry, then merge the change after its pull-request CI passes.
2. From a clean, up-to-date checkout of `main`, validate and push an annotated
   tag matching the version exactly:

   ```bash
   git switch main
   git pull --ff-only origin main
   version="$(cargo run --quiet --locked -p xtask -- release check-cli)"
   tag="cli-v$version"
   cargo run --locked -p xtask -- release check-cli --tag "$tag"
   git tag --annotate "$tag" --message "WF Observer CLI $version"
   git push origin "$tag"
   ```

3. Verify that the tagged Release CLI workflow publishes the GitHub Release
   and updates `OpenGameInterop/scoop-bucket`.

The [Release CLI](.github/workflows/cli-release.yml) workflow validates the tag,
builds and smoke-tests Linux and
Windows x64 executables, packages both with the repository licences and README,
generates SHA-256 checksums, and creates the GitHub Release. Prerelease versions
such as `0.2.0-rc.1` create GitHub prereleases.

Released tags and assets are immutable; publish a new version instead of
replacing an existing release.

## Packaging validation

Run the Release CLI workflow manually against `main` whenever the release
workflow, archive layout, or package templates change. The manual run builds
the complete Linux and Windows artifact set and renders package metadata
without publishing anything.

When the Windows archive or Scoop manifest changes, also install the rendered
manifest locally and exercise installation, service startup, shutdown, update, and
uninstallation. These checks are not required for an ordinary version release.

## Package managers

Stable CLI releases update `wf-observer` in `OpenGameInterop/scoop-bucket`.
Prereleases are not sent to package managers.

The `wf-observer` and `wf-observer-bin` AUR templates are in `packaging/`.
Publication is disabled by the `false` guard on `publish-aur`. Enabling it
requires a configured AUR account and package repositories.

Scoop publication requires `SCOOP_BUCKET_TOKEN`, a fine-grained token with
Contents write access to `OpenGameInterop/scoop-bucket`.

Enabling AUR publication additionally requires `AUR_SSH_PRIVATE_KEY`, containing
a dedicated SSH key registered with the maintainers' AUR account.

## SDK bindings

1. Update the workspace version in `Cargo.toml`, `Cargo.lock`, and the package
   version in `boltffi.toml` together.
2. Run the [local checks](CI.md) and the Release bindings workflow manually.
   Inspect its artifacts and exercise the generated clients before tagging.
3. After merging, push an annotated `v{version}` tag matching both manifests.

The workflow builds native libraries for Linux, macOS, and Windows on x64 and
ARM64. JVM bundles and CPython 3.10–3.14 wheels cover that matrix except Windows
ARM64. C# combines all six targets into one NuGet package using
`boltffi.release.toml`. Apple and browser bindings have separate jobs; Android
packaging is disabled.

Artifacts are uploaded to the workflow run; registry publication is a separate
step. Final bundles are retained for seven days and intermediate C# libraries
for one day. JVM bundles remain target-specific.

For a local release build, use `just binding TARGET --release`, where `TARGET`
is `java`, `csharp`, `python`, `apple`, or `wasm`. Python accepts
`--python PATH`; Apple packaging requires macOS and Xcode.
