WASM_TARGET := wasm32v1-none
WASM := target/$(WASM_TARGET)/release/ourdao_dao.wasm

.PHONY: all build test fmt clippy audit ci clean optimize size costs bindings check coverage

all: check fmt clippy test build

build:
	cargo build --locked --target $(WASM_TARGET) --release

# Small, deterministic release wasm ready for deployment.
optimize: build
	stellar contract build --optimize

test:
	cargo test --locked

# The optimized wasm that gets deployed; `stellar contract build` optimizes by default.
OPTIMIZED := target/optimized/ourdao_dao.wasm

$(OPTIMIZED): build
	stellar contract build --locked --package ourdao-dao --out-dir target/optimized

# Release wasm size, raw and optimized (#357).
size: build $(OPTIMIZED)
	@wc -c $(WASM) $(OPTIMIZED)

# Per-entrypoint resource costs of the optimized wasm (#358).
costs: $(OPTIMIZED)
	OURDAO_WASM=$(CURDIR)/$(OPTIMIZED) cargo test --locked --lib costs -- --ignored --nocapture --test-threads=1

bindings: generate-bindings
	@echo "TypeScript bindings are at bindings/ts/"

check:
	cargo check --locked --all-targets

# Requires `cargo install cargo-llvm-cov --locked` once locally.
coverage:
	cargo llvm-cov --locked --all-targets --fail-under-lines 95

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets --locked -- -D warnings

# Reproduces the full CI sequence locally (minus Docker reproducible build).
# Use this before pushing to confirm CI will pass.
ci: check fmt clippy test coverage build

# Requires `cargo install cargo-audit --locked` once.
audit:
	cargo audit

clean:
	cargo clean
