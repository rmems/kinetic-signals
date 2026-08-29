# Review Guidelines

This document defines code review standards for the `kinetic-signals` crate.

## Review scope

### In-scope files

| Path | What to check |
|------|---------------|
| `src/` | Correctness, performance, API design, docs, tests |
| `tests/` | Test coverage, edge cases, cross-language parity |
| `examples/` | Demo completeness, runnable examples |
| `.github/workflows/` | CI correctness, security (SHA pinning, permissions) |
| `Cargo.toml` | Dependencies, features, version bumps |
| `README.md` | Accuracy, completeness, badges |
| `docs/` | Architecture docs, boundary matrix |
| `AGENTS.md` | Agent instructions accuracy |
| `REVIEW.md` | This file |
| `.codacy.yml` | Exclusion patterns match intent; do not hide security-relevant paths |
| `codecov.yml` | Coverage thresholds and ignore paths |
| `Dockerfile` | Base image, build correctness, multi-stage hygiene |
| `.gitignore` | Ignore rules for build artifacts, IDE/bot dirs, lockfile |

### Out-of-scope

`.beads/`, `.mimocode/`, `.kilo/`, IDE configs (`.idea/`, `.cursor/`, `.vscode/`), `Cargo.lock` (library crate), license text files (`LICENSE-MIT`, `LICENSE-APACHE-2.0`) unless dual-license policy changes.

Paths not listed above default to **in-scope** if they affect build, CI, packaging, or public docs; default to **out-of-scope** if they are local tooling or generated artifacts.

## Bot review rules

| Bot | Focus areas | Common fixes |
|-----|-------------|--------------|
| **Codacy** | Security (SHA pinning), code complexity (excluded: markdown, examples, test fixtures) | Pin actions to SHAs, reduce cyclomatic complexity in Rust code |
| **Devin** | Behavioral consistency, missing caching, MSRV concerns | Add cargo caching, clarify intentional vs accidental behavior |
| **CodeRabbit** | `persist-credentials: false`, least-privilege permissions | Add permissions block, disable credential persistence |
| **Kilo Code** | Suggestions, warnings, code improvements | Address suggestions with rationale or fix |
| **Cursor** | Bug detection, security review | Fix bugs, verify security claims |

### How to handle bot comments

1. **Read the comment** — understand the finding
2. **Verify against current code** — is it still valid?
3. **Fix if actionable** — push a fix, reply with commit SHA
4. **Reply substantively if not fixable** — explain why (don't use empty acknowledgments)
5. **Resolve the thread** — after replying

## Severity levels

| Level | Response expected |
|-------|-------------------|
| **Critical** | Must fix before merge |
| **Major** | Should fix before merge |
| **Minor** | Fix if easy, otherwise document why deferred |
| **Nitpick** | Optional, address at author's discretion |

## Merge criteria

All required for merge:

- [ ] All CI checks passing (no `FAILURE`/`ERROR`/`ACTION_REQUIRED`)
- [ ] `mergeStateStatus` is `CLEAN`
- [ ] `mergeable` is `MERGEABLE`
- [ ] `reviewDecision` is not `CHANGES_REQUESTED`
- [ ] Zero unresolved inline review threads
- [ ] Every bot/human thread either fixed in code or answered substantively

## PR naming conventions

Use [Conventional Commits](https://www.conventionalcommits.org/):

| Prefix | When to use |
|--------|-------------|
| `feat:` | New feature or API |
| `fix:` | Bug fix |
| `docs:` | Documentation only |
| `chore:` | CI, dependencies, tooling |
| `refactor:` | Code restructuring without behavior change |
| `test:` | Adding or updating tests |
| `ci:` | CI/CD changes |

## Breaking changes

Full policy: the "Pre-1.0 SemVer / Stability Policy" section of [`README.md`](README.md).

Public API = everything reachable from the crate root, from a `pub mod`, or
via `prelude`; the Cargo feature names in `[features]`; and every existing
trait implementation on a public type (e.g. `Default`, `Clone`). When a
change to any of those surfaces is breaking (removing or renaming a public
item, changing a signature, tightening a precondition, narrowing a trait
bound, removing an existing trait impl, adding a field to an existing public
struct — none of this crate's public structs are `#[non_exhaustive]` — or
removing/renaming a feature even if everything it gates stays available
under a replacement name; adding an inherent method or a new trait impl,
which can shadow a downstream method or introduce resolution ambiguity; or
adding any new item to a module re-exported by `prelude`, since its glob
re-export can collide with a downstream glob import of the same name):

1. Bump the version (pre-1.0: minor `0.X.0` → `0.(X+1).0`; post-1.0: major `X.Y.Z` → `(X+1).0.0`)
2. Add a migration guide to README (see "Upgrading from v0.3.x" for the format)
3. Update `docs/boundary-matrix.md` if applicable
4. Update `CHANGELOG.md` (once introduced — see the release-documentation issue under #8) with the breaking change under a "Breaking" heading

## Cross-repo handoff

When a review comment or issue belongs to another crate:

| If it's about... | Redirect to |
|-------------------|-------------|
| Spike-train analysis (ISI, PSTH) | SpikeStream.jl |
| SNN runtime / neuron models | neuromod |
| Financial domain adapters | DendriteTrader.jl / metabolic-ledger |
| Hardware signal acquisition | silicon-bridge |
| Supervisory orchestration | brainstem-daemon |
