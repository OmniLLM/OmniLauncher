#!/usr/bin/env bash
# Install a self-contained Python into ~/.omnilauncher/python/
# Uses uv (preferred) or installs uv first as fallback.
# Usage: ./install-bundled-python.sh [python-version]
set -e

DEST="$HOME/.omnilauncher/python"
PYTHON_VERSION="${1:-3.12}"

if [ -f "$DEST/bin/python3" ]; then
  echo "✅ Bundled Python already installed at $DEST"
  "$DEST/bin/python3" --version
  exit 0
fi

mkdir -p "$DEST"

# Ensure uv is available
if ! command -v uv &>/dev/null; then
  echo "📦 uv not found — installing uv..."
  curl -LsSf https://astral.sh/uv/install.sh | sh
  export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
fi

echo "📦 Installing Python $PYTHON_VERSION into $DEST ..."
uv python install "$PYTHON_VERSION" --install-dir "$DEST"

echo ""
echo "✅ Bundled Python installed:"
"$DEST/bin/python3" --version
echo ""
echo "OmniLauncher will now use this Python for all external plugins."
echo "Restart OmniLauncher to apply."
