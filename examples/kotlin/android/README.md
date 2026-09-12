# Android/Kotlin example

Android packaging is disabled in [boltffi.toml](../../../boltffi.toml).

Iroh requires Android-specific JVM and application-context initialization.
Enabling it requires that initialization in the binding and an instrumentation
test of the packaged client.

See [Iroh's Android requirements](https://docs.rs/iroh/latest/iroh/endpoint/struct.Endpoint.html#usage-on-android).
