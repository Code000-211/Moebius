# Examples

Each example is small, runs in CI and is meant to be copied. The contracts they use are in
[`docs/reference/`](../docs/reference/).

- [`scripts/ready-large-cards-alone.luau`](scripts/ready-large-cards-alone.luau): a column script. A Ready script that
  starts a card whose title contains `[large]` only when no run is going. Copy it to `.moebius/ready.luau` on the
  repository's default branch. Tested as a repository override by `crates/moebius-core/tests/examples.rs`. See
  [column scripts](../docs/reference/column-scripts.md).
- [`rules.md`](rules.md): three rules in prose. Point `rules_file` at it, or copy it to `.moebius/rules.md` on the
  default branch. Parsed by the same test. See [rules](../docs/reference/rules.md).
- [`../crates/moebius-plugin-api/examples/allow_reads.rs`](../crates/moebius-plugin-api/examples/allow_reads.rs): a
  decider plugin that allows tools which only read and refuses the rest, run through a `Host`. CI runs it with
  `cargo run -p moebius-plugin-api --example allow_reads`. See [plugin API](../docs/reference/plugin-api.md).
- [`pi-extension/log-tool-calls.ts`](pi-extension/log-tool-calls.ts): a Pi extension that logs every tool call to
  standard error. It is not checked in CI, which has no TypeScript toolchain. Load it by naming its absolute path in an
  agent's `extensions`:

  ```toml
  [agents.Coder]
  model = "my-model"
  extensions = ["/srv/moebius/examples/pi-extension/log-tool-calls.ts"]
  ```

  The harness passes each entry as `--extension` after the bridge, so the bridge still checks every call first. The
  bridge, `crates/moebius-core/bridge/moebius-bridge.ts`, is the full-featured reference; see the
  [bridge protocol](../docs/reference/bridge-protocol.md).
