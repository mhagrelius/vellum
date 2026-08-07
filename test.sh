#!/usr/bin/env bash
#
# Everything CI would run, in the order that fails fastest.
#
#   ./test.sh              against your own session
#   ./test.sh --headless   under Xvfb and a private D-Bus session
#
# The headless mode exists for the widget tests: GTK needs a display, and a
# test run must not attach to the developer's real session bus, where it would
# talk to a live instance of the app instead of itself.

set -euo pipefail

cd "$(dirname "$0")"

headless=false
if [[ "${1:-}" == "--headless" ]]; then
    headless=true
    shift
fi

# Accessibility bridges and the real GSettings backend both reach out to
# session services. Neither is under test, and the second would write the
# developer's own reading style while the suite ran.
export GTK_A11Y=none
export GSETTINGS_BACKEND=memory
export RUST_BACKTRACE=1

# The *schema* still has to exist even with a memory backend, or every
# `Settings::new` aborts. Compiling it into a throwaway directory is also the
# only thing that checks the schema is valid XML with the keys the code reads —
# a typo there is otherwise found by a user, at launch.
work="$(mktemp -d)"
runtime_dir=""
cleanup() {
    rc=$?
    [[ -n "$runtime_dir" ]] && fusermount3 -u "$runtime_dir/doc" 2>/dev/null
    rm -rf "$work" ${runtime_dir:+"$runtime_dir"}
    exit $rc
}
trap cleanup EXIT

export XDG_DATA_HOME="$work/data"
export XDG_CONFIG_HOME="$work/config"
export XDG_STATE_HOME="$work/state"
export XDG_CACHE_HOME="$work/cache"
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"

echo "==> glib-compile-schemas"
mkdir -p "$work/schemas"
cp data/us.hagreli.Vellum.gschema.xml "$work/schemas/"
glib-compile-schemas --strict "$work/schemas"
export GSETTINGS_SCHEMA_DIR="$work/schemas"

# The private bus activates its own xdg-document-portal, which mounts a FUSE fs
# at $XDG_RUNTIME_DIR/doc. Inheriting the login session's runtime dir means that
# mount lands on /run/user/$UID/doc, on top of the real portal's; the real one
# exits 21 and every flatpak launch fails until it is restarted. Hand the
# session a throwaway runtime dir so its portals stay inside it.
if $headless; then
    runtime_dir="$(mktemp -d)"
    chmod 700 "$runtime_dir"
    export XDG_RUNTIME_DIR="$runtime_dir"
fi

run() {
    echo "==> $*"
    if $headless; then
        xvfb-run -a dbus-run-session -- "$@"
    else
        "$@"
    fi
}

# Formatting and lints need no display, so they never go through the wrapper.
echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

# --workspace so vellum-core is tested too. Without it cargo checks only the
# root package, and the half of the suite that needs no display — which is most
# of it — silently stops running.
run cargo test --workspace --all-targets

echo
echo "All green."
