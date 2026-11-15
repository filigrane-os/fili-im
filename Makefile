# Conversations (fili-im). `make check` must pass before every commit.
.PHONY: build release test check fmt

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

fmt:
	cargo fmt

check:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	cargo test
