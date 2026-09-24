#!/usr/bin/env bash
# Rebuild crates/neutron/assets/lucide-subset.ttf from lucide-static.
#
# Usage: scripts/subset-icons.sh [lucide-static version]
#
# The glyph list below must match `Icon` in crates/neutron/src/icons.rs;
# `cargo test -p neutron icons` fails if an Icon's codepoint is missing from
# the bundled font. Needs curl, python3 and fonttools (pyftsubset); under
# Nix: `nix shell nixpkgs#python3Packages.fonttools -c scripts/subset-icons.sh`.
set -euo pipefail

NAMES=(
  undo-2 redo-2 zoom-in zoom-out scan settings search plus check x ellipsis
  chevron-down chevron-up chevron-right link square octagon flag zap trash-2
  rotate-ccw arrow-right command crosshair palette circle-alert
  corner-down-left history
)

root=$(cd "$(dirname "$0")/.." && pwd)
version=${1:-$(curl -s https://registry.npmjs.org/lucide-static/latest |
  python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])')}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

curl -sSfL "https://registry.npmjs.org/lucide-static/-/lucide-static-$version.tgz" |
  tar xz -C "$work"
unicodes=$(python3 - "$work/package/font/codepoints.json" "${NAMES[@]}" <<'PY'
import json, sys
points = json.load(open(sys.argv[1]))
print(",".join("U+%04X" % points[name] for name in sys.argv[2:]))
PY
)
pyftsubset "$work/package/font/lucide.ttf" --unicodes="$unicodes" \
  --no-hinting --desubroutinize \
  --output-file="$root/crates/neutron/assets/lucide-subset.ttf"
cp "$work/package/LICENSE" "$root/crates/neutron/assets/Lucide-LICENSE.txt"
echo "lucide-static $version: ${#NAMES[@]} glyphs"
