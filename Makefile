# OmniLauncher backend build/install Makefile
#
# Runtime operations (start/stop/status/logs/settings/skills/plugins/...) live in
# the self-contained binary CLI (`ol`). Keep Make only for building and managing
# the installed binary/symlinks.

.PHONY: help build build-binary install uninstall install-cli uninstall-cli clean remove-binary

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
	$(info OmniLauncher backend - build/install only)
	$(info )
	$(info   make build       build release backend binary)
	$(info   make install     build and install/symlink binary CLI into PREFIX/bin)
	$(info   make uninstall   remove installed symlinks/copies)
	$(info   make clean       remove Rust build artifacts)
	$(info )
	$(info Runtime management is inside the binary CLI:)
	$(info   ol serve|start|stop|restart|status|health|logs|doctor)
	$(info   ol settings|skills|plugins ...)
	$(info )
	$(info Variables: PREFIX=$(PREFIX) BINDIR=$(BINDIR))
	@:

build: build-binary

build-binary:
	cd src-tauri && $(CARGO) build --release

install: build install-cli

# Install both names: `ol` for CLI muscle memory and `omnilauncher` for direct
# serve/ops invocation from PATH. Symlinks keep rebuilds cheap and avoid copying
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
	rm -rf src-tauri/target target

remove-binary:
	rm -f "$(BIN)"
