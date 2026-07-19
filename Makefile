# CASTERM Makefile
# All cargo invocations execute inside Docker -- never on the host

PROJECTNAME := casterm
PROJECTORG  := casapps
VERSION     := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

DOCKER_IMAGE := casjaysdev/rust:latest
SUFFIX       := $(shell tr -dc 'a-z0-9' </dev/urandom | head -c8)

CARGO_CACHE ?= $(HOME)/.cargo
RUSTUP_CACHE ?= $(HOME)/.rustup
SCCACHE_CACHE ?= $(HOME)/.cache/sccache
CARGO_TARGET ?= $(HOME)/.cache/cargo-target/$(PROJECTNAME)

DOCKER_RUN   := docker run --rm --name "$(PROJECTNAME)-$(SUFFIX)" \
                -v "$(PWD):/work" -w /work \
                -v "$(CARGO_CACHE):/root/.cargo" \
                -v "$(RUSTUP_CACHE):/root/.rustup" \
                -v "$(SCCACHE_CACHE):/root/.cache/sccache" \
                -v "$(CARGO_TARGET):/root/.cache/cargo-target"

TARGETS := x86_64-unknown-linux-musl aarch64-unknown-linux-musl

.PHONY: all build test lint fmt clean help \
        build-x86_64 build-aarch64

all: build

help:
	@echo "CASTERM v$(VERSION)"
	@echo ""
	@echo "Targets:"
	@echo "  build          Build release binary (via Docker, amd64 musl)"
	@echo "  test           Run tests (via Docker)"
	@echo "  lint           Run clippy"
	@echo "  fmt            Format code"
	@echo "  fmt-check      Check formatting"
	@echo "  clean          Clean build artifacts"
	@echo "  build-x86_64   Build x86_64-unknown-linux-musl"
	@echo "  build-aarch64  Build aarch64-unknown-linux-musl"

build: build-x86_64

release: build-x86_64

build-x86_64:
	@mkdir -p $(CARGO_CACHE) $(RUSTUP_CACHE) $(SCCACHE_CACHE) $(CARGO_TARGET)
	$(DOCKER_RUN) $(DOCKER_IMAGE) \
		cargo build --release --target x86_64-unknown-linux-musl

build-aarch64:
	@mkdir -p $(CARGO_CACHE) $(RUSTUP_CACHE) $(SCCACHE_CACHE) $(CARGO_TARGET)
	$(DOCKER_RUN) $(DOCKER_IMAGE) \
		cargo build --release --target aarch64-unknown-linux-musl

test:
	@mkdir -p $(CARGO_CACHE) $(RUSTUP_CACHE) $(SCCACHE_CACHE) $(CARGO_TARGET)
	$(DOCKER_RUN) \
		$(DOCKER_IMAGE) \
		bash -c 'cargo fmt --check && cargo test --workspace --all-features'

lint:
	@mkdir -p $(CARGO_CACHE) $(RUSTUP_CACHE) $(SCCACHE_CACHE) $(CARGO_TARGET)
	$(DOCKER_RUN) $(DOCKER_IMAGE) \
		cargo clippy --workspace --all-targets --all-features -- -D warnings

fmt:
	@mkdir -p $(CARGO_CACHE) $(RUSTUP_CACHE) $(SCCACHE_CACHE) $(CARGO_TARGET)
	$(DOCKER_RUN) $(DOCKER_IMAGE) cargo fmt --all

fmt-check:
	@mkdir -p $(CARGO_CACHE) $(RUSTUP_CACHE) $(SCCACHE_CACHE) $(CARGO_TARGET)
	$(DOCKER_RUN) $(DOCKER_IMAGE) cargo fmt --all --check

clean:
	cargo clean 2>/dev/null || true
