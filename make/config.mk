# -- Configuration ------------------------------------------------------------

NPM          ?= npm
NPX          ?= npx
CARGO        ?= cargo
CARGO_TEST_FLAGS ?= -- --test-threads=1
SERVER_HOST  ?= 0.0.0.0
SERVER_PORT  ?= 1422
BACKEND_URL  ?= http://127.0.0.1:$(SERVER_PORT)

# Canonical command selectors. Lowercase aliases support common CLI usage like
# `make restart role=backend` while keeping uppercase variables canonical.
ROLE         ?= $(if $(role),$(role),both)
KIND         ?= all

# Backend mode: local | wsl | remote
BACKEND_MODE ?= local

# Set REBUILD=1 to force a rebuild before starting.
REBUILD ?= 0

# Set DEBUG=1 or VERBOSE=1 to start binaries with --debug.
DEBUG   ?= 0
VERBOSE ?= 0
