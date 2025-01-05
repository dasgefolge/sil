#!/bin/sh

set -e

git push
ssh gefolge.org env -C /opt/git/github.com/dasgefolge/sil/master git pull
ssh gefolge.org env -C /opt/git/github.com/dasgefolge/sil/master cargo build --release
ssh gefolge.org env -C /opt/git/github.com/dasgefolge/gefolge-websocket/master git pull
ssh gefolge.org env -C /opt/git/github.com/dasgefolge/gefolge-websocket/master cargo build --release --features=ctrlflow
ssh gefolge.org sudo systemctl restart gefolge-websocket
