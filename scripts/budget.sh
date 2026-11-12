#!/bin/bash
set -euo pipefail
# Idle-budget guard: ui must stay tiny and timer-free.
SIZE=$(du -sk ui | cut -f1)
echo "ui size: ${SIZE}KB"
if [ "$SIZE" -gt 100 ]; then
  echo "FAIL: ui/ exceeds 100KB budget"; exit 1
fi
if grep -rn "setInterval\|setTimeout\|requestAnimationFrame" ui/ ; then
  echo "FAIL: ui/ contains timers (idle cost)"; exit 1
fi
echo "budget OK"
