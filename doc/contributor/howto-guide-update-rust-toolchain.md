# How-To Guide: Updating the Rust Toolchain

This guide describes how to update the Rust toolchain version in
`google-cloud-rust` when a new stable Rust compiler is released, and how Minimum
Supported Rust Version (MSRV) updates are managed.

## Overview

The repository tracks two distinct Rust version configurations:

1. **Current Stable Compiler (`rust-toolchain.toml`)**: Updated when the Rust
   project releases a new stable minor release (e.g. `1.99` -> `1.100`). Local
   development, CI checks, formatting, and clippy verification all use the
   toolchain pinned in this file. Only the minor version is pinned, so patch
   releases are picked up without changes.
1. **Minimum Supported Rust Version (MSRV)**: Managed independently under the
   **1-year policy** (`Cargo.toml` `rust-version = "1.XX.0"` and
   `.gcb/msrv.yaml`). Support for an MSRV release is dropped only after 1 year
   has passed since its initial release.

______________________________________________________________________

## Updating the Stable Compiler (6-Week Cycle)

When a new stable compiler is released (or when notified by the automated weekly
toolchain check issue), follow these steps to update the repository.

### Step 1: Run the Toolchain Check Script

Run the automated check script from the repository root:

```bash
./scripts/check-rust-toolchain.sh
```

*(Pass `--force` or an explicit `1.XX` version to force checks when already up
to date.)*

The script will:

1. Fetch `upstream/main` (falling back to `origin/main` / `main`) and compare
   the configured version against the latest release in `RELEASES.md`.
1. Automatically create and check out `chore-bump-rust-toolchain-1.XX` based on
   the latest `main` ref.
1. Update `channel` in `rust-toolchain.toml`, format it with `taplo fmt` (if
   installed), and install the toolchain (`rustup toolchain install`).
1. Run `cargo fmt` across the workspace (including generated crates, since
   [librarian] formats generated output with `cargo fmt`).
1. Run parallel strict workspace clippy verification (`--deny warnings` and
   `cargo clippy-strict`). If clippy fails, it automatically attempts
   `cargo clippy --fix --no-deps` on non-generated crates and re-verifies.
1. Verify `cargo doc` (`-D warnings`) on core crates.
1. Ensure the pinned `cargo-semver-checks` version from
   `.gcb/scripts/semver-checks.sh` and `librarian.yaml` is installed and passes
   on `google-cloud-wkt`.
1. Print a suggested `git commit` command (automatically linking any open
   `"Upgrade Rust toolchain to 1.XX"` issue if `gh` is installed).

### Step 2: Resolve Any Remaining Diagnostics

- **If the script exits `0`:** Review `git status` / `git diff` (which may
  include `cargo fmt` updates or automatic clippy fixes in handwritten crates)
  and proceed to Step 3.
- **If clippy fails on generated code (`**/generated/**`):** **Do not edit
  generated code manually for lint fixes** (`cargo fmt` changes are fine).
  1. File an issue or send a PR in [librarian] to update the generator templates
     first.
  1. Once the updated generator is published and regenerated in
     `google-cloud-rust`, re-run `./scripts/check-rust-toolchain.sh`.
- **If clippy or rustdoc fails on handwritten crates:** Fix the diagnostics
  manually in the handwritten crates (`src/auth`, `src/gax`, `src/storage`,
  etc.) and re-run `./scripts/check-rust-toolchain.sh` until it passes.
- **If `cargo-semver-checks` fails with `unsupported rustdoc format vXX`:**
  Update `cargo-semver-checks` to the latest version in **both**
  `.gcb/scripts/semver-checks.sh` and `librarian.yaml`, then re-run
  `./scripts/check-rust-toolchain.sh` (which will install the updated version
  automatically).

### Step 3: Commit and Submit PR

The `channel` in `rust-toolchain.toml` is the only version that needs to change.
GitHub Actions and Google Cloud Build install the toolchain pinned in that file,
so no CI configuration files need to be updated.

> [!WARNING]
> Do NOT modify MSRV configurations: `.gcb/msrv.yaml`, `.clippy.toml`, or
> `Cargo.toml` (`rust-version`). Those track the MSRV and are updated
> independently under the 1-year policy.

Commit your changes following the commit message guidelines in
[Contributing Guide] (using the command printed by `check-rust-toolchain.sh`,
including `Fixes #NNNN` if an open tracking issue exists):

```bash
git commit -am "chore: update Rust toolchain to 1.XX" \
  -m "Update the stable compiler version in rust-toolchain.toml to 1.XX." \
  -m "Fixes #NNNN"
```

## Updating the Minimum Supported Rust Version (MSRV)

Under our MSRV policy, we can drop support for a Rust version **1 year after its
release date** (checked automatically weekly by
`.github/workflows/rust-toolchain-check.yaml` or manually at
[releases.rs](https://releases.rs/)).

Run `./scripts/bump-msrv.sh` from the repository root to update `Cargo.toml`,
`.clippy.toml`, `.gcb/msrv.yaml`, `README.md`, and
`doc/contributor/howto-guide-set-up-development-environment.md`, and verify
workspace clippy lints:

```bash
git checkout -b chore-bump-msrv-1.XX
./scripts/bump-msrv.sh
git commit -am "chore(ci): update MSRV to 1.XX.0" \
  -m "Update Minimum Supported Rust Version to 1.XX.0 across Cargo.toml, .clippy.toml, CI configurations, and documentation."
```

### Code Clean Ups

Consider applying any code cleanups or adopting standard library features
enabled by the newer MSRV. This is optional and not a required part of bumping
the version.

[contributing guide]: ../../CONTRIBUTING.md
[librarian]: https://github.com/googleapis/librarian
