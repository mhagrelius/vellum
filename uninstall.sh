#!/usr/bin/env bash
#
# Remove everything install.sh put in place. Your documents are left alone —
# Vellum edits files you chose and keeps nothing of its own beside them.

set -euo pipefail

APP_ID=us.hagreli.Vellum
PREFIX="${PREFIX:-$HOME/.local}"

rm -fv "$PREFIX/bin/vellum" \
       "$PREFIX/share/applications/$APP_ID.desktop" \
       "$PREFIX/share/metainfo/$APP_ID.metainfo.xml" \
       "$PREFIX/share/icons/hicolor/scalable/apps/$APP_ID.svg" \
       "$PREFIX/share/icons/hicolor/symbolic/apps/$APP_ID-symbolic.svg" \
       "$PREFIX/share/glib-2.0/schemas/$APP_ID.gschema.xml" \
       "$PREFIX/share/dbus-1/services/$APP_ID.service"

if command -v glib-compile-schemas >/dev/null; then
    glib-compile-schemas "$PREFIX/share/glib-2.0/schemas" 2>/dev/null || true
fi
if command -v gtk4-update-icon-cache >/dev/null; then
    gtk4-update-icon-cache -qtf "$PREFIX/share/icons/hicolor" || true
fi

echo
echo "Removed. Your chosen reading style is still in dconf; clear it with:"
echo "  gsettings reset-recursively $APP_ID"
