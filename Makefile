# Conversations (fili-im). `make check` must pass before every commit.
# Connectors are built next to fili-im (target/<profile>/), where the daemon looks for them first.
#
#   make release                       build everything in release mode (needs Go and a C compiler, and cxx-qt's qmake: QMAKE=/usr/lib/qt6/bin/qmake6)
#   sudo make install                  install under /usr/local (PREFIX=/usr for a system package)
#   make install DESTDIR=pkg PREFIX=/usr   stage into a directory (packagers)
#   make install-data PREFIX=~/.local      everything but the programs (launcher, icon, licences, what the shell reads)
#
# Everything the application brings to the Filigrane desktop lives here and is installed together: the programs, the launcher,
# the icon, and shell/ (the module of the fili-bar, the fallback menu of the bar, the brief of Lia), which ends in
# $(DATADIR)/filigrane/shell/ where `fili-bar link-system` finds it.
PREFIX ?= /usr/local
DESTDIR ?=
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
TARGET_DIR ?= $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),target)
PROGRAMS = fili-im fili-im-whatsapp fili-im-gmessages

.PHONY: run build release test check fmt connectors connectors-release install install-data uninstall

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

install: release install-data
	for p in $(PROGRAMS); do install -Dm755 $(TARGET_DIR)/release/$$p $(DESTDIR)$(BINDIR)/$$p; done

install-data:
	install -Dm644 data/fili-im.desktop $(DESTDIR)$(DATADIR)/applications/fili-im.desktop
	install -Dm644 data/icons/hicolor/scalable/apps/fili-im.svg $(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/fili-im.svg
	install -Dm644 LICENSE $(DESTDIR)$(DATADIR)/licenses/fili-im/LICENSE
	for f in licenses/*; do install -Dm644 $$f $(DESTDIR)$(DATADIR)/licenses/fili-im/$$(basename $$f); done
	mkdir -p $(DESTDIR)$(DATADIR)/filigrane/shell
	cp -r shell/. $(DESTDIR)$(DATADIR)/filigrane/shell/

uninstall:
	for p in $(PROGRAMS); do rm -f $(DESTDIR)$(BINDIR)/$$p; done
	rm -f $(DESTDIR)$(DATADIR)/applications/fili-im.desktop
	rm -f $(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/fili-im.svg
	rm -rf $(DESTDIR)$(DATADIR)/licenses/fili-im
	rm -rf $(DESTDIR)$(DATADIR)/filigrane/shell/fili-bar/messages
	rm -f $(DESTDIR)$(DATADIR)/filigrane/shell/ctl/fili-im.json $(DESTDIR)$(DATADIR)/filigrane/shell/appmenu/fili-im.json $(DESTDIR)$(DATADIR)/filigrane/shell/lia/fili-im.md

test:
	# --release like every Filigrane application: the debug test binaries of the CI container do not link the cxx-qt
	# initialisers of the fili-ui kit (undefined cxx_qt_init_crate_*), the release ones do
	cargo test --release
	scripts/run-qml-tests.sh
	python3 -I scripts/check-i18n.py
	python3 -I scripts/check-shell-menu.py
	python3 -I scripts/fili-api-lint --app .
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
	scripts/run-qml-tests.sh
	FILI_IM_CONNECTOR=$(CURDIR)/target/debug/fili-im-whatsapp cargo test --test conformance

# Development. `make run` starts that build with the QML read from the source folders,
# so after a change of a .qml file just run it again, no build. `make run ARGS=...`.

run:
	@test -x target/debug/fili-im || { echo "build first (make build)" >&2; exit 1; }
	FILI_QML_DIR=$(CURDIR)/qml QT_QUICK_BACKEND=software target/debug/fili-im $(ARGS)
