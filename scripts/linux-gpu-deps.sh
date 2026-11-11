#!/bin/sh
# Installs a software GPU on Debian and Ubuntu for the renderer's GPU tests (crates/render/tests):
# Mesa's lavapipe Vulkan driver, which needs no graphics card. It comes from the distribution,
# which fixes its version for its release, so it is not pinned here.
set -eu
sudo apt-get update
sudo apt-get install --yes --no-install-recommends mesa-vulkan-drivers
