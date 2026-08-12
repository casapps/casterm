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

## Phase 3 — Session resurrection (implemented)

- CRIU-style process checkpoint/restore (re-attach to a live running
  process across a restart) — out of scope; restoring only re-spawns each
  pane's shell in its saved `cwd`, it does not resume the saved `command`.
- Structured state export (e.g. JSON/YAML dump of live session state for
  external tooling) — not implemented.
- Session locking (prevent two processes from concurrently mutating the
  same on-disk session file) — not implemented; `StateManager` has no
  file-lock/advisory-lock around `save_session`/`remove_session`.
- `app::multiplexer::Pane.terminal` field and its `terminal()`/
  `terminal_mut()`/`set_terminal()` accessors were deleted as dead code
  left over from Phase 2 — superseded by `app::pane_runtime::PaneRuntime`,
  which now owns real per-pane terminal state; `multiplexer::Pane` is pure
  layout-tree bookkeeping (id/title) with no other live callers of the
  removed accessors.
- `state::HistoryState` (struct + `new`/`add`/`search`) was deleted as
  unused scaffolding for a command-history subsystem that was never part
  of the 6-phase plan; reintroduce with a real caller (e.g. TUI command
  palette) if that feature is scoped in later.
- `SessionManager`'s API surface was trimmed to only what single-session
  MVP scope uses (`new`, `insert`, `active`, `active_mut`, `remove`).
  `create`, `get`, `get_mut`, `find_by_name`, `list`, and `set_active` were
  deleted as genuinely unused rather than kept behind `#[allow(dead_code)]`
  — reintroduce them when multi-session list/switch commands are built
  (e.g. a `casterm session list` CLI subcommand or a TUI session picker).

## Phase 4 — SSH transport (implemented)

- Multi-hop jump chains — only `jump_hosts[0]` is used; additional hops in
  the chain are not traversed.
- Local/remote/dynamic port forwarding — `ForwardType`/`tcpip_forward` are
  entirely unimplemented; `SshConfig.forwards` is unused data.
- X11 forwarding and agent forwarding — `x11_forwarding`/`agent_forwarding`
  config fields exist but are not wired to any behavior.
- SFTP browser.
- Hardware-key auth (GSSAPI/PKCS11) — `AuthMethod::KeyboardInteractive` and
  `AuthMethod::Gssapi` explicitly return "not yet supported" errors.
- Connection sharing / multiplexed control sockets.
- Auto-reconnect — `SshConfig.reconnect_attempts`/`reconnect_delay` are
  plain data fields with no reconnect loop behind them; the
  `ConnectionState::Reconnecting` variant was removed as dead code rather
  than kept unimplemented. Reintroduce the variant when auto-reconnect is
  built.
- Persistent SSH host directory (saved connection profiles, `casterm ssh
  list/add/remove`) — the old `SshManager` struct (host-directory CRUD) was
  deleted as genuinely unused scaffolding with zero callers; its two useful
  helpers (`validate`, `connection_url`) were kept as free functions in
  `ssh.rs`. Reintroduce a manager type when that CLI/TUI surface is built.
- `--ssh` CLI flag is ssh-agent-auth only — no flags yet for
  password/key-file/jump-host selection from the command line.
- SSH panes are not part of session save/restore — `state::WindowState`/
  `PaneState` doesn't distinguish SSH panes, so a restored session always
  re-spawns local shells even if the original pane was SSH-backed.

## Phase 5 — Serial transport (implemented)

- Break-signal support — the `serialport` crate exposes no break-condition
  API on its `SerialPort` trait; would need a platform-specific ioctl
  (`TIOCSBRK`/`TIOCCBRK` on Unix) added directly.
- Auto-reconnect backoff tuning — `SerialConfig.auto_reconnect`/
  `reconnect_delay` are plain data fields with no reconnect loop behind
  them; `SerialState::Reconnecting` was removed as dead code rather than
  kept unimplemented, mirroring the SSH `Reconnecting` situation above.
  Reintroduce the variant when auto-reconnect is built.
- `Parity::Mark`/`Parity::Space` are rejected at connect time —
  `serialport` has no mark/space parity support on any backend.
- `StopBits::OneAndHalf` is collapsed onto `StopBits::One` when opening the
  device — `serialport` has no distinct 1.5-stop-bit variant.
- Persistent serial device directory (saved connection profiles) — the old
  `SerialManager` struct and `SerialPreset`/`common_presets()` scaffolding
  were deleted as genuinely unused, zero-caller code; `validate` was kept
  as a free function in `serial.rs`. Reintroduce a manager/preset type when
  that CLI/TUI surface is built.
- `--serial` CLI flag is baud-only — no flags yet for choosing data bits,
  parity, stop bits, or flow control from the command line.
- Serial panes are not part of session save/restore — same limitation as
  SSH panes above.

## Phase 6 — GUI (winit + wgpu) (implemented)

- Single-window, single-pane MVP only — no multi-window/multi-pane splits
  in the GUI yet. `app::pane_runtime` already supports it structurally;
  the GUI event loop (`ui::gui::window::GuiApp`) would need a pane tree
  like `TuiApp`'s to drive more than one.
- SSH- and serial-backed GUI panes are out of scope — `spawn_ssh_pane_runtime`/
  `spawn_serial_pane_runtime` exist and are shared-ready, but
  `ui::gui::window::GuiApp` only calls `spawn_pane_runtime` (local shell).
- No embedded font — `ui::gui::font::find_monospace_font_path` does
  best-effort system font discovery (`CASTERM_GUI_FONT_PATH` override, a
  well-known-path list, then a bounded directory walk for anything with
  "mono" in its file name). IDEA.md's "bundle a Nerd Font" stretch goal is
  not implemented; a headless/minimal system (no fonts installed at all)
  will fail to start the GUI.
- `FontConfig.family` is not honored — the GUI always uses whatever
  `font::load_font()` finds; it doesn't try to match the configured family
  name against installed fonts.
- No transparency, background images, quick-terminal global-hotkey
  overlay, or CRT/scanline shader effects — the renderer draws flat
  opaque cell quads only.
- No HiDPI scale-factor query — `ui::gui::window::font_px` assumes a flat
  96 DPI when converting the configured point size to rasterization
  pixels; on a scaled display glyphs will be too small.
- Selection is stream-style (xterm-like) only — no rectangular/block
  selection mode, and no keyboard-driven selection (mouse click-drag only).
- Copy-on-select-release only — no explicit "copy" keybinding, and no
  paste wiring (`arboard` is only used for `set_text`, not `get_text`).
- No scrollback/mouse-wheel handling in the GUI — `WindowEvent::MouseWheel`
  is not handled; the TUI's scrollback viewer has no GUI equivalent yet.
- Glyph atlas is fixed-size (2048x2048) and never grows — once full,
  further never-before-seen glyphs render as blank cells rather than
  evicting/re-packing existing ones.
- Headless wgpu smoke test (`ui::gui::renderer::tests::headless_instance_and_optional_device`)
  treats a missing GPU adapter as a pass, not a failure — the
  `casjaysdev/rust:latest` Docker toolchain image has no GPU/Vulkan driver
  installed by default (`mesa-vulkan-swrast`/lavapipe is available via
  `apk` but not preinstalled), so this test can't assert a real device is
  always obtainable in CI.

## Cross-cutting

- GUI window icon — `assets::get_icon()` was deleted as dead code with no
  backing asset; reintroduce once an actual icon file is committed to
  `assets/icons/` and wired to `winit::window::Window::set_window_icon`.
- `assets/config/default.yaml` schema has drifted from `config::Config`'s
  actual field names (e.g. `shell.program`/`shell.env`/`shell.login_shell`
  vs. the real `ShellConfig{path, args, login}`) — `assets::default_config()`
  was deleted as dead code rather than wired in with a broken schema. Needs
  a decision: fix `default.yaml` to match `Config`'s real shape and wire it
  into `Config::load()`'s no-config-found fallback, or delete the file
  entirely if it's not meant to be consumed.
- Linux-specific display-server/systemd detection (`platform::linux::Linux`)
  was deleted as genuinely unused scaffolding with zero callers —
  reintroduce if a future feature needs to distinguish Wayland vs. X11
  (e.g. clipboard backend selection) or gate systemd-specific integration
  (e.g. a user service unit).

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
