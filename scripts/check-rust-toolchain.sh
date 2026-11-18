#!/usr/bin/env bash
# Copyright 2026 Google LLC
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     https://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

set -euo pipefail

TOOLCHAIN_FILE="rust-toolchain.toml"

if [[ ! -f "${TOOLCHAIN_FILE}" || ! -f "Cargo.toml" ]]; then
  echo "Error: ${TOOLCHAIN_FILE} or Cargo.toml not found. Run this script from the repository root." >&2
  exit 1
fi

FORCE=false
EXPLICIT_VERSION=""
for arg in "$@"; do
  case "${arg}" in
    --force)
      FORCE=true
      ;;
    [0-9]*.[0-9]*)
      # Normalize 1.XX.0 -> 1.XX if a patch version was passed.
      EXPLICIT_VERSION=$(echo "${arg}" | sed -nE 's/^([0-9]+\.[0-9]+)(\.[0-9]+)?$/\1/p')
      if [[ -z "${EXPLICIT_VERSION}" ]]; then
        echo "Error: Invalid version format '${arg}'. Expected 1.XX." >&2
        exit 1
      fi
      ;;
    *)
      echo "Usage: $0 [1.XX] [--force]" >&2
      exit 1
      ;;
  esac
done

# The `1.XX` version currently in the working tree's `rust-toolchain.toml`.
CURRENT_VERSION=$(sed -nE 's/^channel *= *"([0-9]+\.[0-9]+).*"/\1/p' "${TOOLCHAIN_FILE}" | sed -n '1p')
if [[ ! "${CURRENT_VERSION}" =~ ^[0-9]+\.[0-9]+$ ]]; then
  echo "Error: Could not parse a valid 1.XX channel version from ${TOOLCHAIN_FILE}." >&2
  exit 1
fi
echo "Current working tree Rust toolchain: ${CURRENT_VERSION}"

if [[ -n "${EXPLICIT_VERSION}" ]]; then
  LATEST_VERSION="${EXPLICIT_VERSION}"
  echo "Target Rust release (explicit):      ${LATEST_VERSION}"
else
  LATEST_MINOR=$(curl -fsSL https://raw.githubusercontent.com/rust-lang/rust/master/RELEASES.md | sed -nE 's/^Version 1\.([0-9]+)\..*/\1/p' | sed -n '1p')
  if [[ ! "${LATEST_MINOR}" =~ ^[0-9]+$ ]]; then
    echo "Error: Failed to extract latest stable minor version from rust-lang RELEASES.md." >&2
    exit 1
  fi
  LATEST_VERSION="1.${LATEST_MINOR}"
  echo "Latest stable Rust release:          ${LATEST_VERSION}"
fi

# Resolve the canonical base branch (`upstream/main` -> `origin/main` -> `main`)
# so we branch off fresh `main` and do not falsely exit 0 when re-running on an
# in-progress toolchain bump branch.
BASE_REF="main"
if git remote get-url upstream >/dev/null 2>&1; then
  if git fetch --quiet upstream main 2>/dev/null; then
    BASE_REF="upstream/main"
  elif git show-ref --verify --quiet refs/remotes/upstream/main; then
    BASE_REF="upstream/main"
  fi
elif git remote get-url origin >/dev/null 2>&1; then
  if git fetch --quiet origin main 2>/dev/null; then
    BASE_REF="origin/main"
  elif git show-ref --verify --quiet refs/remotes/origin/main; then
    BASE_REF="origin/main"
  fi
fi

BASE_VERSION=$(git show "${BASE_REF}:${TOOLCHAIN_FILE}" 2>/dev/null | sed -nE 's/^channel *= *"([0-9]+\.[0-9]+).*"/\1/p' | sed -n '1p' || true)
if [[ -n "${BASE_VERSION}" ]]; then
  echo "Configured on ${BASE_REF}:         ${BASE_VERSION}"
fi

TARGET_BRANCH="chore-bump-rust-toolchain-${LATEST_VERSION}"
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)

if [[ "${BASE_VERSION:-${CURRENT_VERSION}}" == "${LATEST_VERSION}" && "${CURRENT_BRANCH}" != "${TARGET_BRANCH}" && "${FORCE}" != "true" ]]; then
  echo "Rust toolchain is already up to date on ${BASE_REF} (${LATEST_VERSION}). Pass --force to run checks anyway."
  exit 0
fi

echo ""
echo "==== 1. Checking out Feature Branch ===="
if [[ "${CURRENT_BRANCH}" != "${TARGET_BRANCH}" ]]; then
  if [[ -n "$(git status --porcelain)" ]]; then
    echo "Error: Working tree has uncommitted changes on '${CURRENT_BRANCH}'. Commit or stash them before switching to '${TARGET_BRANCH}'." >&2
    exit 1
  fi
  if git show-ref --verify --quiet "refs/heads/${TARGET_BRANCH}"; then
    echo "Switching to existing branch: ${TARGET_BRANCH}"
    git checkout "${TARGET_BRANCH}"
  else
    echo "Creating and checking out branch: ${TARGET_BRANCH} (based on ${BASE_REF})"
    git checkout -b "${TARGET_BRANCH}" "${BASE_REF}"
  fi
else
  echo "Already on branch: ${TARGET_BRANCH}"
fi

echo ""
echo "==== 2. Updating ${TOOLCHAIN_FILE} to ${LATEST_VERSION} ===="
# Write to a temporary file instead of using `sed -i`, which is not portable
# between GNU and BSD (macOS) sed.
sed -E "s/^(channel *= *\").*(\")/\1${LATEST_VERSION}\2/" "${TOOLCHAIN_FILE}" >"${TOOLCHAIN_FILE}.tmp"
mv "${TOOLCHAIN_FILE}.tmp" "${TOOLCHAIN_FILE}"

if ! grep -Eq "^channel *= *\"${LATEST_VERSION//./\\.}\"$" "${TOOLCHAIN_FILE}"; then
  echo "Error: Failed to update channel to ${LATEST_VERSION} in ${TOOLCHAIN_FILE}." >&2
  exit 1
fi

if command -v taplo >/dev/null 2>&1; then
  taplo fmt "${TOOLCHAIN_FILE}"
fi

rustup toolchain install
rustc --version
cargo --version

echo ""
echo "==== 3. Formatting Workspace (cargo fmt) ===="
# Run `cargo fmt` before clippy so any rustfmt changes from the new toolchain
# are applied deterministically (including in generated code, since librarian
# formats generated output with `cargo fmt`).
cargo fmt

run_workspace_clippy() {
  # Exclude the two largest generated crates from the workspace pass and check
  # them separately, matching `.gcb/scripts/lint.sh`.
  cargo clippy --all-features --all-targets --profile=test --workspace \
    --exclude google-cloud-compute-v1 \
    --exclude google-cloud-aiplatform-v1 \
    -- --deny warnings
  cargo clippy --all-features --all-targets --profile=test \
    -p google-cloud-compute-v1 -- --deny warnings
  cargo clippy --all-features --all-targets --profile=test \
    -p google-cloud-aiplatform-v1 -- --deny warnings
  cargo clippy-strict
}

echo ""
echo "==== 4. Running Strict Workspace Clippy ===="
if ! run_workspace_clippy; then
  echo ""
  echo "Clippy reported warnings/errors. Attempting machine-applicable fixes on non-generated crates..."
  PKG_ARGS=()
  while IFS= read -r member_dir; do
    if [[ -n "${member_dir}" && -f "${member_dir}/Cargo.toml" ]]; then
      pkg=$(sed -nE 's/^name *= *"([^"]+)".*/\1/p' "${member_dir}/Cargo.toml" | sed -n '1p')
      if [[ -n "${pkg}" ]]; then
        PKG_ARGS+=("-p" "${pkg}")
      fi
    fi
  done < <(sed -n '/^members = \[/,/^\]/p' Cargo.toml | sed -nE 's/^ *"([^"]+)".*/\1/p' | grep -vE '(^|/)generated(/|$)')

  if [[ ${#PKG_ARGS[@]} -gt 0 ]]; then
    cargo clippy --fix --allow-dirty --allow-staged --no-deps --all-features --all-targets --profile=test "${PKG_ARGS[@]}" || true
    cargo fmt
  fi

  echo ""
  echo "Re-running strict workspace clippy after automatic fixes..."
  if ! run_workspace_clippy; then
    echo "" >&2
    echo "ERROR: Strict workspace clippy failed." >&2
    echo "If the diagnostics above are in **/generated/**, generated code cannot be edited directly:" >&2
    echo "  1. Submit a PR in googleapis/librarian to update the generator templates." >&2
    echo "  2. Once librarian regenerates google-cloud-rust, re-run ./scripts/check-rust-toolchain.sh." >&2
    echo "If the diagnostics are in handwritten crates, fix them manually and re-run ./scripts/check-rust-toolchain.sh." >&2
    exit 1
  fi
fi

echo ""
echo "==== 5. Verifying Documentation Builds ===="
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features \
  -p google-cloud-wkt \
  -p google-cloud-rpc \
  -p google-cloud-gax \
  -p google-cloud-auth \
  -p google-cloud-gax-internal \
  -p google-cloud-location

echo ""
echo "==== 6. Verifying Semver Checks Tooling ===="
GCB_SEMVER_VERSION=$(sed -nE 's/.*cargo-semver-checks@([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' .gcb/scripts/semver-checks.sh | sed -n '1p')
LIBRARIAN_SEMVER_VERSION=$(awk '/name: *cargo-semver-checks/ { found=1; next } found && /version:/ { gsub(/["'"'"']/, "", $2); print $2; exit }' librarian.yaml)

if [[ -z "${GCB_SEMVER_VERSION}" || -z "${LIBRARIAN_SEMVER_VERSION}" ]]; then
  echo "Error: Could not parse cargo-semver-checks version from .gcb/scripts/semver-checks.sh or librarian.yaml." >&2
  exit 1
fi

if [[ "${GCB_SEMVER_VERSION}" != "${LIBRARIAN_SEMVER_VERSION}" ]]; then
  echo "Error: cargo-semver-checks version mismatch between .gcb/scripts/semver-checks.sh (${GCB_SEMVER_VERSION}) and librarian.yaml (${LIBRARIAN_SEMVER_VERSION})." >&2
  exit 1
fi

INSTALLED_SEMVER_VERSION=$(cargo semver-checks --version 2>/dev/null | awk '{print $2}' || true)
if [[ "${INSTALLED_SEMVER_VERSION}" != "${GCB_SEMVER_VERSION}" ]]; then
  echo "Installing cargo-semver-checks@${GCB_SEMVER_VERSION} (currently installed: ${INSTALLED_SEMVER_VERSION:-none})..."
  cargo install --locked "cargo-semver-checks@${GCB_SEMVER_VERSION}"
fi

cargo semver-checks --all-features -p google-cloud-wkt

ISSUE_ARG=""
if command -v gh >/dev/null 2>&1; then
  ISSUE_NUMBER=$(gh issue list --repo googleapis/google-cloud-rust \
    --search "Upgrade Rust toolchain to ${LATEST_VERSION} in:title" \
    --state open --json number --jq '.[0].number' 2>/dev/null || true)
  if [[ "${ISSUE_NUMBER}" =~ ^[0-9]+$ ]]; then
    ISSUE_ARG=" -m \"Fixes #${ISSUE_NUMBER}\""
  fi
fi

echo ""
echo "==== Toolchain bump to ${LATEST_VERSION} verified! ===="
echo "Modified files:"
git status --short
echo ""
echo "Suggested commit command:"
echo "  git commit -am \"chore: update Rust toolchain to ${LATEST_VERSION}\" -m \"Update the stable compiler version in rust-toolchain.toml to ${LATEST_VERSION}.\"${ISSUE_ARG}"
