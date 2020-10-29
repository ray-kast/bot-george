#!/bin/sh

set -e

docker-compose up -d db

cargo build

if tmux has-session -t bot-george; then
  tmux send-keys -t bot-george 'C-c'
else
  tmux new-session -d -s bot-george
fi

tmux send-keys -t bot-george 'RUST_BACKTRACE=1 cargo run' 'C-m'
