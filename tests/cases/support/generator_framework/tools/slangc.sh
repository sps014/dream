#!/bin/sh
# Stands in for a registry tool: reads a sibling file from its own install tree.
root="$(dirname "$0")/.."
case "$1" in
    slow) sleep 1 ;;
    medium) sleep 0.5 ;;
esac
printf '%s:%s:%s' "$1" "$(cat input.txt)" "$(cat "$root/lib/sibling.txt")" > "$1.txt"
