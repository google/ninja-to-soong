#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -xe

[ $# -eq 1 ]
DEST="$1"
SCRIPT_DIR="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"

bash "${SCRIPT_DIR}/../checkout.sh" https://github.com/KhronosGroup/OpenCL-CTS 9feccbb8fcb8eefeebf86a48036173a8155f9d21 "${DEST}/external/OpenCL-CTS"
