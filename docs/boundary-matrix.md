# kinetic-signals — Boundary Matrix

Tracked by: Linear LIM-9 | GitHub #10

## Purpose

Domain-agnostic streaming feature extraction for stochastic signals. Computes real-time statistics, point-process intensity, and anomaly metrics on high-velocity time-series data.

**Not** a spike-train analyzer, financial domain adapter, or SNN runtime.

## Owns

| Area | What belongs here |
|------|-------------------|
| Hurst exponent | Long-memory / persistence detection (R/S method) |
| Hawkes process | Self-exciting point-process intensity estimation |
| Surprise | Normalized log-ratio z-score anomaly detection |
| Volatility | Rolling ring-buffer RMS volatility |
| Shannon entropy | Signal complexity / information density |
| Indicators | EMA, SMA, Z-score tracking |
| Signal stats | High-order moments (skewness, kurtosis) |

## Does NOT own

| Area | Belongs to |
|------|------------|
| Spike-train specific analysis (ISI, PSTH, raster) | SpikeStream.jl |
| SNN runtime / neuron models / synaptic dynamics | neuromod |
| Financial domain adapters (portfolio, PnL, Greeks) | DendriteTrader.jl / metabolic-ledger |
| Hardware signal acquisition | silicon-bridge |
| Supervisory orchestration | brainstem-daemon |
| Domain-specific anomaly policies | Application-layer crates |

## Dependencies

### Allowed

| Dependency | Reason |
|------------|--------|
| `temp-env` (dev) | Safe env var testing |
| `serial_test` (dev) | Serial test execution (prevents race conditions in env var tests) |

### Forbidden (by design)

| Dependency | Why forbidden |
|------------|---------------|
| `neuromod` | Would create circular dependency; kinetic-signals is leaf crate |
| `SpikeStream` bindings | Julia crate, different language boundary |
| Financial domain crates | Out of scope for generic signal processing |
| Heavy ML frameworks | Keeps crate zero-required-dependency |

## Cross-repo handoff points

| Consumer | Integration |
|----------|-------------|
| SpikeStream.jl | Uses `compute_hurst`, `compute_hawkes`, `compute_surprise` via FFI or Julia bindings. Shares `tests/fixtures/shared_vectors.json` for cross-language parity. |
| neuromod | May re-export `compute_hurst` / `compute_hawkes` for SNN signal analysis. No direct dependency — re-export only. |
| DendriteTrader.jl | Can use surprise / volatility for financial signal detection. Domain adapter layer lives in DendriteTrader, not here. |

## Thread-safety

All public types are `Send + Sync`. `VolEstimator` requires `&mut self` for mutation (fields: `Vec<f32>`, `usize`, `bool`), so concurrent writes are prevented by the borrow checker. All compute functions are stateless and safe for parallel use.

## Domain leaks / migration risks

- **Deprecated GBM aliases:** Planned for removal in v0.4.0 (PR #17). Once merged, the README migration table will also be removed.
- **SpikeStream.jl transitional proxies:** SpikeStream.jl issues #8, #9, #11 reference financial proxy functions that pointed to kinetic-signals. These proxies should be updated to use the domain-agnostic names (`compute_surprise`, `SurpriseParams`) after v0.4.0.
- Future domain-specific features (e.g., financial Greeks, spike ISI) should be added in consumer crates, not here.
- If a feature is requested that requires domain knowledge, redirect to the appropriate consumer crate.

## Sequencing

1. PR #12 (dual-license) — merged
2. PR #16 (README dev section) — open
3. PR #17 (remove deprecated aliases) — open
4. This document (PR #19) — open
5. crates.io publishing — deferred until repo quality is satisfactory
