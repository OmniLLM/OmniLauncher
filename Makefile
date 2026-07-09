# OmniLauncher build/install Makefile
#
# Runtime operations (start/stop/status/logs/settings/skills/plugins/...) live in
# the self-contained binary CLI (`ol`). Keep Make only for building and managing
# the installed binary/symlinks.

.PHONY: help build build-frontend build-binary install uninstall install-cli uninstall-cli clean remove-binary

NPM ?= npm
CARGO ?= cargo
PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
BIN := src-tauri/target/release/omnilauncher
OL := $(BINDIR)/ol
OMNILAUNCHER := $(BINDIR)/omnilauncher

ifeq ($(OS),Windows_NT)
  BIN := src-tauri/target/release/omnilauncher.exe
  OL := $(BINDIR)/ol.exe
  OMNILAUNCHER := $(BINDIR)/omnilauncher.exe
endif

help:
	$(info OmniLauncher - build/install only)
	$(info )
	$(info   make build       build frontend assets + release binary)
	$(info   make install     build and install/symlink binary CLI into PREFIX/bin)
	$(info   make uninstall   remove installed symlinks/copies)
	$(info   make clean       remove build artifacts)
	$(info )
	$(info Runtime management is inside the binary CLI:)
	$(info   ol start|stop|restart|status|logs|doctor)
	$(info   ol settings|skills|plugins ...)
	$(info )
	$(info Variables: PREFIX=$(PREFIX) BINDIR=$(BINDIR))
	@:

build: build-frontend build-binary

build-frontend:
	$(NPM) run build

build-binary:
	cd src-tauri && $(CARGO) build --release

install: build install-cli

# Install both names: `ol` for CLI muscle memory and `omnilauncher` for direct
# GUI/serve invocation from PATH. Symlinks keep rebuilds cheap and avoid copying
# large binaries around during development.
install-cli:
	@mkdir -p "$(BINDIR)"
	@ln -sf "$(CURDIR)/$(BIN)" "$(OL)"
	@ln -sf "$(CURDIR)/$(BIN)" "$(OMNILAUNCHER)"
	@echo "linked $(OL) -> $(BIN)"
	@echo "linked $(OMNILAUNCHER) -> $(BIN)"
	@case ":$$PATH:" in *":$(BINDIR):"*) ;; *) echo "warning: $(BINDIR) is not on your PATH";; esac

uninstall: uninstall-cli

uninstall-cli:
	@rm -f "$(OL)" "$(OMNILAUNCHER)"
	@echo "removed $(OL) and $(OMNILAUNCHER)"

clean:
	rm -rf dist src-tauri/target

remove-binary:
	rm -f "$(BIN)"
