#!/usr/bin/env bash
#
# Install Vellum for the current user. No root, nothing outside $HOME.
#
#   ./install.sh              build and install
#   ./install.sh --uninstall  the same as ./uninstall.sh

set -euo pipefail
cd "$(dirname "$0")"

if [[ "${1:-}" == "--uninstall" ]]; then
    exec ./uninstall.sh
fi

APP_ID=us.hagreli.Vellum
PREFIX="${PREFIX:-$HOME/.local}"

echo "==> Building"
cargo build --release --locked

echo "==> Installing to $PREFIX"
install -Dm755 target/release/vellum "$PREFIX/bin/vellum"
install -Dm644 "data/$APP_ID.desktop" "$PREFIX/share/applications/$APP_ID.desktop"
install -Dm644 "data/$APP_ID.metainfo.xml" \
    "$PREFIX/share/metainfo/$APP_ID.metainfo.xml"
install -Dm644 "data/icons/hicolor/scalable/apps/$APP_ID.svg" \
    "$PREFIX/share/icons/hicolor/scalable/apps/$APP_ID.svg"
install -Dm644 "data/icons/hicolor/symbolic/apps/$APP_ID-symbolic.svg" \
    "$PREFIX/share/icons/hicolor/symbolic/apps/$APP_ID-symbolic.svg"

# The reading style, the appearance and the window size live in GSettings, so
# the schema has to be installed and compiled. Without it the app still opens —
# it falls back to its defaults — but it forgets everything between launches.
install -Dm644 "data/$APP_ID.gschema.xml" \
    "$PREFIX/share/glib-2.0/schemas/$APP_ID.gschema.xml"
if command -v glib-compile-schemas >/dev/null; then
    glib-compile-schemas "$PREFIX/share/glib-2.0/schemas" \
        || echo "warning: could not compile the schema; settings will not persist" >&2
else
    echo "warning: glib-compile-schemas not found; settings will not persist" >&2
fi

# The desktop file says DBusActivatable, so the session needs a service file
# pointing at the binary or activation fails and the launcher does nothing.
install -Dm644 /dev/stdin "$PREFIX/share/dbus-1/services/$APP_ID.service" <<EOF
[D-BUS Service]
Name=$APP_ID
Exec=$PREFIX/bin/vellum --gapplication-service
EOF

if command -v gtk4-update-icon-cache >/dev/null; then
    gtk4-update-icon-cache -qtf "$PREFIX/share/icons/hicolor" || true
fi
if command -v update-desktop-database >/dev/null; then
    update-desktop-database -q "$PREFIX/share/applications" || true
fi

echo
echo "Installed. Run 'vellum FILE.md', or find it in your applications."
if [[ ":$PATH:" != *":$PREFIX/bin:"* ]]; then
    echo "Note: $PREFIX/bin is not on your PATH."
fi
