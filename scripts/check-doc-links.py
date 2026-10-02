#!/usr/bin/env python3
"""Check that every relative link in a tracked Markdown file resolves.

Extracted from the CI workflow when the project moved off GitHub Actions
(ADR 0019). Fenced code blocks are skipped, because a link inside an example is
an example, not a link. Remote URLs and mail links are ignored.
"""

import os
import re
import sys

SKIP_DIRS = {".git", "target", "node_modules"}

broken = 0
for root, dirs, files in os.walk("."):
    dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
    for name in files:
        if not name.endswith(".md"):
            continue
        path = os.path.join(root, name)
        with open(path, encoding="utf-8") as handle:
            lines, fenced = handle.read().split("\n"), False
        buf = []
        for line in lines:
            if line.strip().startswith("```"):
                fenced = not fenced
                continue
            if not fenced:
                buf.append(line)
        for match in re.finditer(r"\]\(([^)#][^)]*)\)", "\n".join(buf)):
            link = match.group(1)
            if link.startswith(("http", "mailto:")):
                continue
            target = os.path.normpath(os.path.join(root, link.split("#")[0]))
            if not os.path.exists(target):
                print(f"{path} links to a missing file: {link}")
                broken += 1

sys.exit(1 if broken else 0)
