# Sentry integration guide

Optional error monitoring via the [Sentry Rust SDK](https://docs.sentry.io/platforms/rust/). The integration is feature-gated and does nothing unless both the `sentry` feature and a DSN are present.

## Setup

### 1. Enable the feature

Add the feature flag in your `Cargo.toml`:

```toml
[dependencies]
kinetic-signals = { git = "https://github.com/rmems/kinetic-signals", features = ["sentry"] }
```

Or when building this crate directly:

```bash
cargo build --features sentry
# or
cargo run --example demo --features sentry
```

This pulls in the optional `sentry` dependency (currently `0.48.2`).

### 2. Set `SENTRY_DSN`

```bash
export SENTRY_DSN=https://<key>@<org>.ingest.sentry.io/<project>
```

`init_sentry()` reads this environment variable at runtime. If it is unset or empty, initialization is a no-op and returns `None`. No events are sent without a DSN.

## API

### `init_sentry()`

```rust
// Gated on kinetic-signals feature "sentry" (compile-time in THIS crate):
// #[cfg(feature = "sentry")]
pub fn init_sentry() -> Option<sentry::ClientInitGuard>
```

Only compiled into `kinetic-signals` when **its** `sentry` feature is enabled.
Downstream crates get the symbol by enabling `kinetic-signals/sentry` (see Usage).

| Behavior | Result |
|----------|--------|
| `SENTRY_DSN` set and non-empty | `Some(guard)` — Sentry client initialized |
| `SENTRY_DSN` unset or empty | `None` — no client, no network traffic |

The returned `ClientInitGuard` **must live for the program lifetime**. Keep it in a binding (e.g. `let _guard = ...`) so it is not dropped early. When the guard is dropped, Sentry flushes pending events (up to 2 seconds).

### Release name

`init_sentry()` configures the client with:

```rust
sentry::ClientOptions {
    release: sentry::release_name!(),
    ..Default::default()
}
```

`sentry::release_name!()` expands to `CARGO_PKG_NAME@CARGO_PKG_VERSION`, e.g. `kinetic-signals@0.4.0`. This string is attached to every event so Sentry can group issues by release.

## Usage example

Downstream crates that enable `kinetic-signals/sentry` can call `init_sentry`
unconditionally (the symbol only exists when that feature is on). Prefer a
**forwarding feature** in the binary's `Cargo.toml` rather than relying on a
local feature name of `"sentry"` alone:

```toml
# binary / app Cargo.toml
[features]
sentry = ["kinetic-signals/sentry"]

[dependencies]
# Prefer git until crates.io publish lands (#8 / LIM-193):
kinetic-signals = {
  git = "https://github.com/rmems/kinetic-signals",
  features = ["sentry"],
}
# After publish: kinetic-signals = { version = "0.4", features = ["sentry"] }
```

```rust
fn main() {
    // Keep the guard alive for the whole process so events flush on exit.
    // Available only when THIS crate's build enables kinetic-signals/sentry
    // (e.g. `cargo run --features sentry` with a forwarding feature, or when
    // kinetic-signals is always built with `features = ["sentry"]` as above).
    let _guard = kinetic_signals::init_sentry();

    // ... application code ...
}
```

If `kinetic-signals` is a dependency of a library that does not re-export a
`sentry` feature, call sites that live behind `#[cfg(feature = "…")]` must use
**your** feature name that forwards to `kinetic-signals/sentry` — a bare
`#[cfg(feature = "sentry")]` in the consumer only works when that crate defines
a `sentry` feature itself.

Demo binary (same pattern):

```bash
SENTRY_DSN=https://...@... cargo run --example demo --features sentry
```

## Release tracking

Tagged releases are registered in Sentry automatically via
[`.github/workflows/sentry-release.yml`](../.github/workflows/sentry-release.yml).

| Detail | Value |
|--------|--------|
| Trigger | Push of tags matching `v*` |
| Version transform | Strip leading `v` from the tag (`v0.4.0` → `0.4.0`) |
| Sentry version | `kinetic-signals@X.Y.Z` |
| Org / project | `limen-neural` / `kinetic-signals` |
| Environment | `production` |

When the git tag matches `Cargo.toml` version (e.g. tag `v0.4.0` and package version `0.4.0`), the workflow version `kinetic-signals@0.4.0` matches `sentry::release_name!()` from builds of that version. Events reported by those binaries then associate with the correct Sentry release.

Required secret: `SENTRY_AUTH_TOKEN` (repository secrets).

## Privacy and defaults

- Sentry is **never** initialized unless the `sentry` feature is enabled **and** `SENTRY_DSN` is present and non-empty.
- No data is sent by default; the feature is opt-in at both compile time and runtime.
- Integration tests live in [`tests/sentry_feature.rs`](../tests/sentry_feature.rs) (run with `cargo test --features sentry`).
