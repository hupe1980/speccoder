# speccoder — development task runner (https://just.systems). `just ci` mirrors CI.

# Publish order: every crate after the crates it depends on.
crates := "speccoder-core speccoder-speckit speccoder-runtime speccoder"

default:
    @just --list --unsorted

# Every quality gate, in order; stops at the first failure.
ci: fmt-check lint test deny layering doc package

fmt:
    cargo fmt --all

fmt-check:
    @echo "==> gate: fmt-check"
    cargo fmt --all --check

lint:
    @echo "==> gate: lint"
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    @echo "==> gate: test"
    cargo nextest run --workspace --all-features -E 'not binary(layering)'
    cargo test --workspace --doc --all-features

deny:
    @echo "==> gate: deny"
    cargo deny check

# Layering rules of the architecture (crates/speccoder/tests/layering.rs).
layering:
    @echo "==> gate: layering"
    cargo nextest run -p speccoder --test layering

doc:
    @echo "==> gate: doc"
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# Release dry run: package every crate and check each ships licences and readme.
package:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "==> gate: package"
    cargo package --workspace --no-verify --allow-dirty --quiet
    for crate in {{crates}}; do
        files=$(cargo package -p "$crate" --list --allow-dirty)
        for f in LICENSE-APACHE LICENSE-MIT README.md; do
            grep -qx "$f" <<<"$files" || { echo "package: $crate is missing $f" >&2; exit 1; }
        done
        echo "package: $crate ok"
    done

# Serve the documentation site with live reload (needs zola).
site:
    cd site && zola serve

# Build the site and check every link, internal and external.
site-check:
    cd site && zola check && zola build

# Reserve the library crate names on crates.io with 0.0.1 placeholders.
# Irreversible and outward-facing: asks first, never runs in CI, not part of `ci`.
[confirm("Publish placeholder 0.0.1 of speccoder-core, speccoder-speckit, speccoder-runtime to crates.io? [y/N]")]
reserve-names:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "${CI:-}" ]; then echo "reserve-names refuses to run in CI" >&2; exit 1; fi
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    for crate in speccoder-core speccoder-speckit speccoder-runtime; do
        dir="$tmp/$crate"
        mkdir -p "$dir/src"
        cp LICENSE-APACHE LICENSE-MIT "$dir/"
        printf '# %s\n\nPart of [speccoder](https://github.com/hupe1980/speccoder). This release only reserves the name.\n' "$crate" > "$dir/README.md"
        printf '//! Part of speccoder; this release only reserves the name.\n' > "$dir/src/lib.rs"
        printf '[package]\nname = "%s"\nversion = "0.0.1"\nedition = "2024"\nlicense = "MIT OR Apache-2.0"\nrepository = "https://github.com/hupe1980/speccoder"\ndescription = "Part of speccoder, a spec-driven coding agent for GitHub Spec Kit (name reservation)"\nreadme = "README.md"\n\n[workspace]\n' "$crate" > "$dir/Cargo.toml"
        (cd "$dir" && cargo publish --allow-dirty)
    done

# Generate a Spec Kit compatibility fixture with one release, then
# add it to SUPPORTED in crates/speccoder-runtime/tests/compat.rs.
compat-fixture version:
    scripts/speckit-fixture.sh {{version}}

