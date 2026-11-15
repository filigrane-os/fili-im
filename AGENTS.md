# Conversations (fili-im): working agreements

Read before changing anything:

| Area | Read |
|---|---|
| Any work | [docs/plan.md](docs/plan.md): architecture, phases, which model does what |
| Connectors, capabilities, ids | [docs/connector-protocol.md](docs/connector-protocol.md): the contract |
| Window, bar module (P2, P3) | Filigrane's `agents/skills/filigrane-ui/references/app-window.md`; copy patterns from Facteur (`filigrane-os/fili-mail`) and Casier (`filigrane-os/fili-file`) |

## Rules

- Everything in this repository is in English (code, comments, docs, commits, CLI messages, UI strings).
- `make check` passes before every commit. A fix comes with a test.
- The core never speaks a platform protocol; the window and the bar never test a platform name. Platform differences
  are capabilities (`src/protocol.rs`), checked by `Capabilities::allows` before a command leaves the daemon.
- A protocol change updates, together: `docs/connector-protocol.md`, `src/protocol.rs`, `fili-im-fake`, and
  `tests/conformance.rs`. Additions stay within protocol 1; only incompatible changes bump it.
- Message contents, keys and tokens never go to logs, argv or error messages.
- Messages, names and file names are untrusted: plain-text rendering in the window.

## Pitfalls already met

- **Store lock and temporaries**: `match store().x() { … }` and `if let … = store().x() { … }` keep the store's
  mutex guard alive for the whole block; calling `chat_changed` (which locks again) inside deadlocks. Bind first:
  `let r = store().x(); match r { … }`.
- **Envelope keys**: command parameters must never be named `id` or `cmd` (they collide with the envelope and the
  reply never comes back). `Connector::command` refuses them.
- **Socket path length**: a Unix socket path is limited to 108 bytes; test directories must stay short
  (`/tmp/fim-…`).
- **Supervisor task**: it is the only reader of a connector's stdout, so code running in it must not await a reply
  from that connector (spawn a task, as `flush` does).
