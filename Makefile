# Conversations (fili-im). `make check` must pass before every commit.
# Connectors are built next to fili-im (target/<profile>/), where the daemon looks for them first.
#
#   make release                       build everything in release mode (needs Go and a C compiler, and cxx-qt's qmake: QMAKE=/usr/lib/qt6/bin/qmake6)
#   sudo make install                  install under /usr/local (PREFIX=/usr for a system package)
#   make install DESTDIR=pkg PREFIX=/usr   stage into a directory (packagers)
PREFIX ?= /usr/local
DESTDIR ?=
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
TARGET_DIR ?= $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),target)
PROGRAMS = fili-im fili-im-whatsapp fili-im-gmessages

.PHONY: build release test check fmt connectors connectors-release install uninstall

GO_BUILD = CGO_ENABLED=1 go build -trimpath -ldflags="-s -w"

build: connectors
	cargo build

release: connectors-release
	# the packaged build skips fili-im-fake (a development tool that needs no GUI objects)
	cargo build --release --locked --bin fili-im

connectors:
	mkdir -p target/debug
	cd connectors/whatsapp && $(GO_BUILD) -o ../../target/debug/fili-im-whatsapp .
	cd connectors/gmessages && $(GO_BUILD) -o ../../target/debug/fili-im-gmessages .

connectors-release:
	mkdir -p $(TARGET_DIR)/release
	cd connectors/whatsapp && $(GO_BUILD) -o $(abspath $(TARGET_DIR))/release/fili-im-whatsapp .
	cd connectors/gmessages && $(GO_BUILD) -o $(abspath $(TARGET_DIR))/release/fili-im-gmessages .

install: release
	for p in $(PROGRAMS); do install -Dm755 $(TARGET_DIR)/release/$$p $(DESTDIR)$(BINDIR)/$$p; done
	install -Dm644 data/fili-im.desktop $(DESTDIR)$(DATADIR)/applications/fili-im.desktop
	install -Dm644 data/icons/hicolor/scalable/apps/fili-im.svg $(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/fili-im.svg
	install -Dm644 LICENSE $(DESTDIR)$(DATADIR)/licenses/fili-im/LICENSE
	for f in licenses/*; do install -Dm644 $$f $(DESTDIR)$(DATADIR)/licenses/fili-im/$$(basename $$f); done

uninstall:
	for p in $(PROGRAMS); do rm -f $(DESTDIR)$(BINDIR)/$$p; done
	rm -f $(DESTDIR)$(DATADIR)/applications/fili-im.desktop
	rm -f $(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/fili-im.svg
	rm -rf $(DESTDIR)$(DATADIR)/licenses/fili-im

test:
	cargo test
	cd connectors/whatsapp && go test ./...
	cd connectors/gmessages && go test ./...

fmt:
	cargo fmt
	cd connectors/whatsapp && gofmt -w .
	cd connectors/gmessages && gofmt -w .

check: build
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	test -z "$$(cd connectors/whatsapp && gofmt -l .)"
	cd connectors/whatsapp && go vet ./... && go test ./...
	test -z "$$(cd connectors/gmessages && gofmt -l .)"
	cd connectors/gmessages && go vet ./... && go test ./...
	cargo test
	FILI_IM_CONNECTOR=$(CURDIR)/target/debug/fili-im-whatsapp cargo test --test conformance
