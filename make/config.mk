# -- Configuration ------------------------------------------------------------

NPM          ?= npm
NPX          ?= npx
CARGO        ?= cargo
CARGO_TEST_FLAGS ?= -- --test-threads=1
SPLIT_HOST   ?= 0.0.0.0
SPLIT_PORT   ?= 1422
BACKEND_URL  ?= http://127.0.0.1:$(SPLIT_PORT)

# Canonical command selectors.
ROLE         ?= both
KIND         ?= all

# Backend mode: local | wsl | remote
BACKEND_MODE ?= local

# Set REBUILD=1 to force a rebuild before starting.
REBUILD ?= 0

# Set DEBUG=1 or VERBOSE=1 to start binaries with --debug.
DEBUG   ?= 0
VERBOSE ?= 0
