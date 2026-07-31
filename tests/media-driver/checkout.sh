#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 1 ]
DEST="$1"
SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"

bash "${SCRIPT_DIR}/../checkout.sh" https://github.com/intel/media-driver.git 39a3e8273c7c2dd60916d4c75a4cb3db1f92b622 "${DEST}/vendor/intel/desktop/media-driver"

sudo apt-get update
sudo apt install libtool libdrm-dev xorg xorg-dev openbox libx11-dev libgl1 libglx-mesa0
