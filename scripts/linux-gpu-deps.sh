#!/bin/sh
# Installs a software GPU on Debian and Ubuntu for the renderer's GPU tests (crates/render/tests):
# Mesa's lavapipe Vulkan driver, which needs no graphics card, and the Vulkan loader wgpu opens.
# They come from the distribution, which fixes their versions for its release, so they are not
# pinned here.
set -eu
sudo apt-get update
sudo apt-get install --yes --no-install-recommends libvulkan1 mesa-vulkan-drivers
