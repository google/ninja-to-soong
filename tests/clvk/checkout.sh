#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 1 ]
DEST="$1"
SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"

bash "${SCRIPT_DIR}/../checkout.sh" https://github.com/kpet/clvk c821386d4a46a28e3357831f5527627b1ff1b2a8 "${DEST}/external/clvk"
