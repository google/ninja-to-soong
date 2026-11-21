#!/usr/bin/env bash
# Copyright 2026 ninja-to-soong authors
# SPDX-License-Identifier: Apache-2.0

set -e

[ $# -eq 1 ]

NDK_PATH="$1"

python3 -c 'import json, sys; print(int(json.load(open(sys.argv[1]))["max"]))' "${NDK_PATH}/meta/platforms.json"
