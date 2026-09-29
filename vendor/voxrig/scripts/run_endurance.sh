#!/usr/bin/env bash
set -euo pipefail

mc_port="${MC_PORT:-25566}"
mkdir -p reports
cargo build --release --example endurance

MC_PORT="$mc_port" BOT_COUNT=1 BOT_PREFIX=FinalOne DURATION_SECS=3600 \
  REPORT_PATH=reports/1bot-1hour.md target/release/examples/endurance
MC_PORT="$mc_port" BOT_COUNT=10 BOT_PREFIX=FinalTen DURATION_SECS=1800 \
  REPORT_PATH=reports/10bots-30min.md target/release/examples/endurance
MC_PORT="$mc_port" BOT_COUNT=50 BOT_PREFIX=FinalFifty DURATION_SECS=600 \
  REPORT_PATH=reports/50bots-10min.md target/release/examples/endurance
