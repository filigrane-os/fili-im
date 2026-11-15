# Conversations (fili-im). `make check` must pass before every commit.
# Connectors are built next to fili-im (target/<profile>/), where the daemon looks for them first.
.PHONY: build release test check fmt connectors connectors-release

GO_BUILD = CGO_ENABLED=1 go build -trimpath -ldflags="-s -w"

build: connectors
	cargo build

release: connectors-release
	cargo build --release

connectors:
	mkdir -p target/debug
	cd connectors/whatsapp && $(GO_BUILD) -o ../../target/debug/fili-im-whatsapp .

connectors-release:
	mkdir -p target/release
	cd connectors/whatsapp && $(GO_BUILD) -o ../../target/release/fili-im-whatsapp .

test:
	cargo test
	cd connectors/whatsapp && go test ./...

fmt:
	cargo fmt
	cd connectors/whatsapp && gofmt -w .

check: build
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	test -z "$$(cd connectors/whatsapp && gofmt -l .)"
	cd connectors/whatsapp && go vet ./... && go test ./...
	cargo test
	FILI_IM_CONNECTOR=$(CURDIR)/target/debug/fili-im-whatsapp cargo test --test conformance
