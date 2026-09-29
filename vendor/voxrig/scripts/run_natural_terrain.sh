#!/usr/bin/env bash
set -euo pipefail

mc_port="${MC_PORT:-25567}"
bot_count="${BOT_COUNT:-10}"
duration_secs="${DURATION_SECS:-600}"

mkdir -p reports
cargo build --release --example endurance

MC_PORT="$mc_port" \
BOT_COUNT="$bot_count" \
BOT_PREFIX=Natural \
DURATION_SECS="$duration_secs" \
ALLOW_SERVER_TELEPORTS=true \
SCENARIO=natural-terrain-peaceful \
SERVER_TELEPORT_INTERVAL_SECS=180 \
WANDER=true \
REPORT_PATH=reports/natural-terrain.md \
target/release/examples/endurance
