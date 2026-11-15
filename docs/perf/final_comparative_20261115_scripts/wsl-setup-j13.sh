#!/bin/bash
# Prepares the WSL2 guest for the registered J13 study: user-space node and jq, a clean checkout of the measured commit.
set -euo pipefail
SHA="$1"
mkdir -p "$HOME/tools/bin" "$HOME/work"
if [ ! -x "$HOME/tools/bin/node" ]; then
  curl -fsSL "https://nodejs.org/dist/v24.22.2/node-v24.22.2-linux-x64.tar.gz" -o "$HOME/tools/node.tgz"
  tar -xzf "$HOME/tools/node.tgz" -C "$HOME/tools"
  ln -sf "$HOME/tools/node-v24.22.2-linux-x64/bin/node" "$HOME/tools/bin/node"
fi
if [ ! -x "$HOME/tools/bin/jq" ]; then
  curl -fsSL -o "$HOME/tools/bin/jq" "https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-linux-amd64"
  chmod +x "$HOME/tools/bin/jq"
fi
export PATH="$HOME/tools/bin:$HOME/.cargo/bin:$PATH"
node --version
jq --version
rustc --version
if [ ! -d "$HOME/work/sefer-j13/.git" ]; then
  git clone --no-checkout https://github.com/PHPCraftdream/sefer-alloc.git "$HOME/work/sefer-j13"
fi
cd "$HOME/work/sefer-j13"
git fetch -q origin
git checkout -q --detach "$SHA"
git rev-parse HEAD
git status --porcelain | head -3
echo SETUP-OK
