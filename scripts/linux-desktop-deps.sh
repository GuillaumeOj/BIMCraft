#!/bin/sh
# Installs the system libraries the desktop window builds against on Debian and Ubuntu: GTK 3 and
# WebKitGTK 4.1, which Tauri 2 uses on Linux (docs/dev/desktop.md). They come from the
# distribution, which fixes their versions for its release, so they are not pinned here.
set -eu
sudo apt-get update
sudo apt-get install --yes --no-install-recommends libwebkit2gtk-4.1-dev
