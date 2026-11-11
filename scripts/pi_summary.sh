#!/bin/bash
# P-I: prints scripts/pi_report.py (cycles and instructions) for every docs/bench-pi-*.json  ->  docs/bench-pi-summary.txt
cd "$(dirname "$0")/.."
for f in docs/bench-pi-*.json; do
  python3 scripts/pi_report.py "$f"
  python3 scripts/pi_report.py "$f" instructions_min
  echo
done
