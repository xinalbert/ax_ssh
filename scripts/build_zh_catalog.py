#!/usr/bin/env python3
"""Preserve the existing Chinese catalog command using the shared PO workflow."""

from __future__ import annotations

import sys

from build_translations import main

if __name__ == "__main__":
    raise SystemExit(main(["--locale", "zh-CN", *sys.argv[1:]]))
