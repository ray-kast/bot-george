#!/bin/sh

set -e
cd "$(dirname "$0")"

docker-compose up -d db

cargo +nightly build --release

if tmux has-session -t bot-george; then
  tmux send-keys -t bot-george 'C-c'
else
  tmux new-session -d -s bot-george
fi

tmux send-keys -t bot-george 'RUST_BACKTRACE=1 cargo +nightly run --release' 'C-m'
