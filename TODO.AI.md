# TODO.AI.md

Follow-up items logged per project convention ("no issue left only in
conversation"). Remove each line only once fully implemented.

## Phase 1 — Keybindings (implemented)

- Per-profile keybinding sets (different bindings per connection profile).
- IME-suppression during composition (avoid dispatching partial IME input
  as keybinding chords).
- Human-readable modifier naming (e.g. "Ctrl+Space") in a future
  config-editor UI, distinct from the internal `C-Space`-style spec syntax.

## Phase 2 — Multiplexer wiring (implemented)

- Floating/stacked pane layouts (beyond binary horizontal/vertical splits).
- Pane zoom (temporarily maximize one pane, restoring the split layout on
  unzoom).
- Broadcast mode (mirror keystrokes to every pane in a window at once).
- Break/join panes (move a pane out into its own window, or merge two
  windows' panes back together).
- Tree-mode session browser (visual window/pane tree navigator).
- DBus remote control of the multiplexer (per IDEA.md's stretch scope).
- Saved/loadable layout files (name and restore a specific split
  arrangement independent of session resurrection).
- `next-window` / `prev-window` keybinding actions currently no-op: Phase 2
  is single-window-per-session (MVP scope per the approved plan); real
  multi-window support inside one session is deferred to whichever future
  phase adds `Window` collections instead of a single `Window` per `TuiApp`.
- Architecture note (not a defect, logged for traceability): the approved
  plan's Phase 2 wording said "wire `App` to own a `Window`/multiplexer per
  active session ... give `Session` a real `Window`." The actual
  implementation wires `Window` directly onto `TuiApp` in `ui/tui/mod.rs`
  instead, since `crate::app::App`/`crate::app::session::Session` have no
  other live callers yet and Phase 3 (session resurrection) will need to
  restructure this ownership anyway once `StateManager` save/restore needs
  a `Session` to own the `Window` for serialization. `App`/`Session`/
  `SessionManager` remain intentionally unconstructed dead code until
  Phase 3 wires them in.

## Audit findings carried forward (not yet fixed)

- `aws-lc-sys` pulled into the dependency tree via `rustls`'s default
  crypto backend. Fix: pin
  `rustls = { version = "0.23", default-features = false, features =
  ["ring", "std", "tls12", "logging"] }` in Cargo.toml, then verify with
  `cargo tree` that `aws-lc-sys` is gone.
- `libudev-sys` pulled into the dependency tree via `serialport`. Fix:
  `serialport = { version = "4", default-features = false }`, then verify
  static linkage with `ldd` against the built binary. Relevant to the
  Phase 5 serial-transport work.
- `src/main.rs`: `--version` is missing the conventional `-v` short flag.
  Fix: `#[arg(short = 'v', long, action = clap::ArgAction::Version)]`.
- `renovate.json` missing from repo root — deferred until CI/CD work
  starts (blocked by the standing "no CI/CD until fully implemented"
  constraint).
- `.dockerignore` missing from repo root.
- `Makefile:6` — VERSION derivation should prefer `release.txt` over
  grepping `Cargo.toml` (cosmetic).
- `SPEC.md:52-56` references Go for the web-client section of a Rust-only
  project — needs a user decision on the Rust templating mechanism before
  this can be corrected (SPEC.md is user-owned).
- `SPEC.md:15` — config path conflict: `~/.config/casterm/custom.yml` vs.
  the actual `~/.config/casapps/casterm/` path used by `config::Config`.
  Needs a user decision on whether this is intentional or an oversight.
- `serde_yaml = "0.9"` is deprecated/unmaintained — needs a migration
  target decision (e.g. `serde_yaml_ng`, `serde_norway`); not urgent.
- `src/platform/mod.rs`'s own `cache_dir()` (lines 32-33) lacks the
  `casapps/` org prefix present in `config::Config::cache_dir()` —
  inconsistency between the two cache-dir implementations, needs
  reconciling.
