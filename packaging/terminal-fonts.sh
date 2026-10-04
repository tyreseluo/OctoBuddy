#!/bin/sh
# The fonts OctoBuddy's terminal draws with (src/lib.rs, AgentTerm), into
# ./dist/resources for cargo-packager. robius-packaging-commands keeps only
# the fonts the binary's font manifest names, and the terminal's are named
# at run time, so it leaves these two out. Run after it.
set -eu
dir_of() {
  cargo metadata --format-version 1 --locked | python3 -c '
import json, os, sys
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == sys.argv[1]:
        print(os.path.dirname(p["manifest_path"]))
        break
' "$1"
}
copy() {
  src="$(dir_of "$1")/resources/$2"
  dest="dist/resources/$(echo "$1" | tr - _)/resources"
  if [ ! -f "$src" ]; then
    echo "terminal-fonts: $1 has no resources/$2" >&2
    exit 1
  fi
  mkdir -p "$dest"
  cp "$src" "$dest/"
  echo "terminal-fonts: $1/resources/$2"
}
copy makepad-widgets jetbrains_mono_variable.ttf
copy makepad-terminal SymbolsNerdFontMono-Regular.ttf
