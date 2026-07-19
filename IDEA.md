## Project description

CASTERM is a modern terminal emulator, system console replacement, multiplexer, and SSH/serial connection manager — all in a single self-contained binary. It is designed to be the only terminal a user ever needs, from a bare framebuffer console at boot to a full GPU-accelerated GUI window.

On the emulator side it replaces: Ghostty, Alacritty, Kitty, WezTerm, iTerm2, Windows Terminal, Warp, Foot, Rio, Konsole, XFCE4 Terminal, Terminator, Tabby, Hyper, Tilix, GNOME Terminal, xterm, rxvt/urxvt, Guake, Yakuake, Tilda, and every other standalone terminal emulator a developer might reach for.

On the multiplexer side it replaces: tmux, Zellij, Byobu, and GNU Screen — including GNU Screen's multiuser and ACL features.

On the SSH/connection manager side it replaces: PuTTY, Tera Term, MobaXterm, Termius, Xshell, ZOC, SecureCRT, mRemoteNG, and similar tools — including their host directories, SSH key management, port forwarding managers, SFTP browsers, serial-port support, and macro/scripting capabilities.

It ships 200+ features that normally require external plugins, shell integrations, or helper utilities — all built in, with no external dependencies.

## Project variables

project_name: casterm
project_org: casapps
internal_name: casterm
internal_org: casapps
binary: casterm
version: 1.0.0
license: MIT
language: Rust
repository: https://github.com/casapps/casterm

## Business logic

### Core non-negotiables
- Must work with zero configuration out of the box
- Must ship as a single self-contained binary with no external runtime dependencies
- Must run on Windows, macOS, Linux, and BSD without modification
- Must automatically detect its environment and select the appropriate mode without a manual flag
- Must provide a universal status bar that works without any shell integration or plugin installation
- Must support Unicode and wide characters (CJK, emoji, combining characters) correctly
- Must NOT have a plugin system — all features are built in to the binary; no third-party extensions

### Operating modes
CASTERM must run correctly in all of the following modes, auto-detected from environment:

- **GUI window** — native window with full GPU-accelerated rendering; replaces Ghostty, Alacritty, Kitty, WezTerm, iTerm2, Windows Terminal, Warp, Foot, Rio, Konsole, XFCE4 Terminal, Terminator, Tabby, Hyper, Tilix
- **TUI** — runs inside an existing terminal (SSH, nested, CI); full multiplexer available; replaces tmux, Zellij, Byobu
- **Login terminal** — runs as the userspace login session handler, replacing getty/agetty on Linux and the equivalent on macOS and BSD; provides a modern login experience without requiring a display server
- **System console** — runs directly on the Linux framebuffer (or equivalent on other platforms) without X11 or Wayland; replaces the kernel virtual terminal for users who want a modern console experience at boot; must work without a display server being present

### Shell detection and selection
- The user's configured default shell takes highest priority; if it is unset or the binary does not exist, fall back to OS detection
- Shell must be stored in configuration as a full absolute path so the user knows exactly which binary is in use
- Prefer `/usr/local/bin` and Homebrew-managed paths over system paths when both exist
- If no shell is configured, detect and prefer in this order by platform:
  - **Linux / BSD**: system default from `/etc/passwd` or `getpwuid()` → `$SHELL` env → bash → zsh → fish → sh
  - **macOS**: Homebrew bash (if installed, full path) → system default from `getpwuid()` → zsh → bash → sh
  - **Windows**: Git Bash / WSL bash (if installed, full path) → PowerShell (`pwsh`) → `cmd.exe`
- Must validate the detected shell exists and is executable before using it; skip invalid entries and continue down the list
- Must record the resolved full path in the generated config on first run

### Multiplexer requirements
- Must support sessions, windows, and panes with a tmux-compatible prefix-key workflow; feature parity with tmux 3.x and Zellij
- Must allow multiple clients to attach to the same session simultaneously; a client may attach in read-only mode (observe without sending input) or in read-write mode
- Must support per-user access control on shared sessions: the session owner must be able to grant or revoke read-only or read-write access for specific connecting users
- Must support multiuser ACL groups — group users who share common access rights via a group leader
- Must support ACL umask — default permissions for windows created by specific users
- Must support writelocks — explicit exclusive write access mode (on/off/auto) to prevent simultaneous input conflicts
- Must support one-command session sharing with a unique access token that a remote collaborator can use to attach over the network (tmate-style)
- Must support nested sessions (local inside SSH inside container) with automatically shifted prefix keys so inner and outer multiplexers never conflict
- Must support broadcast mode — simultaneously sending input to multiple panes
- Must support all standard layout types: even, main-vertical, main-horizontal, grid, and custom
- Must support copy mode — keyboard navigation through the scrollback buffer with visual selection; selection modes must include character, word, line, and rectangular (block); rectangular selection copies only the selected columns without including surrounding line content
- Must support both vi-style and emacs-style keybindings in copy mode, user-selectable
- Must support pane zoom — temporarily expand the active pane to fill the full terminal, restoring the prior layout on un-zoom
- Must support breaking a pane out into its own window and joining a pane from another window into the current one
- Must support window auto-naming — automatically rename a window to reflect the foreground process (e.g. "vim:main.rs", "ssh:prod")
- Must support window and pane tagging and notes for user organization
- Must support floating panes — overlay panes that hover above the current layout without displacing it; a floating pane can be summoned and dismissed with a single keybinding (scratchpad behavior), sized and positioned freely, and multiple floating panes must be supported simultaneously
- Must support named paste buffers — multiple independent named clipboard slots in addition to the system clipboard; buffers persist for the lifetime of the session and must be listable, selectable, and renameable
- Must support named paste registers — single-character named registers for quick access (vi-style)
- Must support reading and writing paste buffers to/from filesystem files
- Must support clipboard history with disk persistence — clipboard history must survive application restarts
- Must support window linking — linking two or more windows so they share the same pane layout and mirror input; linked windows display in different sessions and are useful for monitoring or paired workflows
- Must support window groups — a named collection of windows that can be acted on together; windows can be added to or removed from a group at any time
- Must support stacked panes — multiple panes layered in the same screen area without consuming additional horizontal or vertical space; the user navigates between stacked panes by keyboard; stacked panes are distinct from floating panes (which overlay all content) and from split panes (which divide the screen physically)
- Must support command panes — a first-class pane type that runs a specific command, displays the command's exit code after it completes, and allows single-keystroke re-execution of the same command without closing or recreating the pane
- Must support selecting multiple panes simultaneously and applying an action to all selected panes at once (close, move to window, stack, float, synchronize input)
- Must support respawning a dead pane or window in place — restarting a pane whose process has exited without changing its position in the layout, its working directory, or its assigned command; this replaces the manual close-and-recreate workflow
- Must support zombie mode — optionally keep dead windows visible with a keystroke to resurrect; useful for reviewing final output before dismissal
- Must support new panes inheriting current working directory from the active pane by default
- Must support pipe-pane — continuously pipe all output from a specific pane to an external command for live logging, processing, or monitoring without interrupting the pane
- Must support capture-pane — programmatically export the current visible content or full scrollback of a pane to a named buffer or file without interrupting the running process; must be scriptable
- Must support programmatic send-keys — inject arbitrary keystrokes, including control sequences and special keys, into any pane from a script, another pane, or an external process

### Session resurrection
- Sessions must be automatically saved continuously so they survive both clean exits and unclean system crashes
- On startup, casterm must automatically detect and offer to restore any previously running sessions without requiring user action beyond confirmation (or restore silently if configured to do so)
- Resurrection must restore: session names, window names and order, pane layout, working directory per pane, and the command that was running in each pane (re-executed in the same working directory)
- **Standard resurrection (all platforms)**: re-execute the last foreground command in each pane so the user lands back in the same program; this is always available
- **Checkpoint/restore resurrection (Linux, where kernel and capability support is available)**: use process checkpointing to snapshot actual process state — memory, file descriptors, execution position — and restore it exactly; this is a platform-specific enhancement that must be attempted automatically and must degrade silently to standard resurrection when unavailable
- Resurrection state must be written to persistent storage (never tmpfs or in-memory only) so it survives a power loss

### Session management
- Must provide an interactive tree-mode browser for all sessions, windows, and panes — a navigable picker that shows the full session hierarchy and allows switching, renaming, and killing entries from a single view
- Must show a welcome screen / session picker at startup when no session is specified — listing existing sessions, a create-new option, and available saved layouts; the welcome screen must be skippable in configuration
- Must support session locking — a locked session hides all content and blocks input until the user authenticates; locking must be triggerable manually and optionally auto-triggered after a configurable idle timeout; the unlock mechanism must use a user-configured passphrase or delegate to the system's PAM/keychain
- Must support creating and controlling sessions entirely from external scripts without attaching — create, destroy, rename, and send input to sessions from a shell process or CI runner that has no terminal attached; this is the foundation for startup scripting and automation
- Must support active sessions monitor/dashboard — a view showing connection status, health, latency, and state across all open sessions
- Must support tab groups — organize tabs/windows into named groups for visual separation
- Must support tab/window tiling — tile sessions side-by-side for comparison within the same window
- Must support tab detach to new window — drag a tab out of the tab bar to spawn a new window
- Must support middle-click to close tab
- Must support "warn on close with running processes" — prompt before closing a tab/window if child processes are still active

### Terminal compatibility
- Must be compatible with interactive terminal applications (vim, nvim, htop, less, man, etc.) without requiring them to be reconfigured
- Must support true color (24-bit), mouse reporting, bracketed paste, alternate screen, OSC hyperlinks, and focus events (FocusIn/FocusOut)
- Must support inline image display via the Sixel, Kitty, and iTerm2 image protocols
- Must support inline video playback — play video files directly in the terminal pane
- Must support inline PDF preview — view PDF documents directly in the terminal
- Must support inline SVG rendering — render SVG files in the terminal
- Must support Tektronix 4014 graphics mode — vector graphics emulation for legacy plotting software
- Must support VT52/VT100/VT102/VT220/VT320 emulation level selection — runtime-switchable emulation affecting escape sequence interpretation
- Must support double-width and double-height line display (VT100 DECDHL/DECDWL) for banner text
- Must support modifyOtherKeys mode — the xterm extension allowing applications to distinguish modified key combinations (e.g. Ctrl+A vs Ctrl+Shift+A)
- Must support OSC 7 (current working directory reporting) and OSC 99 (desktop notifications with buttons/actions)
- Must support synchronized rendering (DECSET 2026) — defer redraws until output batch is complete to prevent screen tearing

### Input method support (IME)
- Must support input method editors for composing CJK and other complex-script text on all platforms
- On Linux, must integrate with IBus and Fcitx and any IME framework that exposes an XIM or Wayland text-input interface
- On macOS, must integrate with the system Input Sources framework
- On Windows, must integrate with the Windows Input Method Framework
- Pre-edit (in-progress composition) text must be displayed inline in the active pane with a clear visual distinction from committed text
- Committed text must be delivered to the pane exactly as if it had been typed directly
- IME composition state must not conflict with the casterm prefix key or any other casterm keybinding; casterm keybindings must be suppressed while an IME composition is in progress

### Scrollback
- Must retain a configurable amount of scrollback history per pane
- Scrollback must be searchable and navigable in copy mode

### Status bar
- Must detect the current VCS, shell, working directory, virtual environment, last exit code, and command duration without shell hooks
- Must show VCS branch and status information for all supported VCS types — not only Git; every VCS type gets the same first-class status bar treatment
- Must support all 20+ VCS types: Git, Mercurial, Subversion, Fossil, Bazaar, Darcs, Pijul, CVS, Perforce, ClearCase, TFS, Plastic, ArX, Monotone, SCCS, RCS, BitKeeper, Aegis, AccuRev, SourceSafe

#### Mode indicator (leftmost element)
The mode indicator is always the leftmost component. Its label and color change based on the current mode:

| State | Label | Default color |
|-------|-------|---------------|
| Normal (no mode active) | Active shell name, uppercase abbreviated (BASH, ZSH, FISH, PWSH, NU, CMD, SH, …) | Theme default — no special highlight |
| Prefix active | PRE | Green |
| Copy mode | COPY | Yellow |
| Broadcast active | BCAST | Red |
| Zen mode | ZEN | Purple |
| Command prompt | CMD | Cyan |
| Search active | SRCH | Orange |

- In normal state the indicator acts as a live shell label rather than a static app name — the user always knows which shell is running in the active pane
- When any mode activates the label is replaced by the mode name and the component background or text color changes to the mode color; when the mode exits it returns to the shell label and default color
- All mode colors are user-configurable
- Must support a prompt character widget — a single character or emoji that indicates the last command's exit status; default: 😇 (success) / 😔 (failure); both characters are user-configurable; the prompt character is optional and distinct from the mode indicator
- The shell label is derived from the active shell binary name, not from `$SHELL` (which reflects the login shell, not the current one); if the name cannot be determined it falls back to the binary's basename uppercased
- Must support optional system widgets: battery level, memory usage, CPU/load average, system uptime, date/time, and weather
- Must support language/runtime version widgets: display active versions of PHP, Ruby, Node, Python, Rust, Go, Java, Perl, Lua, .NET, and other languages when in a project directory that uses that language; versions are detected from version managers (asdf, nvm, pyenv, rbenv, rustup, etc.) or system binaries; widgets appear only when relevant (e.g., Node version only in directories containing package.json)
- Must support virtual environment indicators: Python venv/virtualenv/conda, Node nvm/fnm, Ruby rvm/rbenv, and other language-specific environment managers; display the environment name when active
- Must support container/orchestration indicators: Docker container status, Kubernetes context, Terraform workspace, Vagrant status when in relevant project directories
- Must support left, center, and right layout sections with user-configurable component placement
- A component that represents unavailable hardware or an unconfigured service must be silently omitted — no placeholder, no empty separator (e.g. no battery widget if no battery exists, no weather if no location is available, no VCS info if not in a repository, no virtual environment indicator if none is active)
- The default separator between components is `│` (U+2502 BOX DRAWINGS LIGHT VERTICAL); the separator character must be user-configurable
- Separators between components must only render when both neighboring components are actually present; a separator must never appear adjacent to a silently-omitted component

#### Status bar responsive layout

The status bar uses seven named width breakpoints. All breakpoints and component sets below are the **default** behavior; users may override any component at any breakpoint in configuration.

**Breakpoint definitions (terminal columns):**

| Name    | Columns   | Typical context                              |
|---------|-----------|----------------------------------------------|
| nano    | < 60      | Phone portrait, Termux, extreme narrow       |
| tiny    | 60 – 79   | Phone landscape, very small terminal window  |
| small   | 80 – 119  | Classic 80-col, tablet portrait, small window|
| medium  | 120 – 159 | Laptop terminal, typical SSH session         |
| large   | 160 – 199 | Wide laptop, standard desktop window         |
| xlarge  | 200 – 239 | Ultrawide, large desktop, multi-pane layouts |
| xxlarge | ≥ 240     | 4K ultrawide, very wide single window        |

**Default component sets per breakpoint:**

The left section always begins with the mode indicator. In the layouts below "SHELL" means the shell label (e.g. BASH) in normal state, or the mode label (e.g. PRE) when a mode is active. The `│` separator is shown explicitly to illustrate which components appear at each breakpoint.

*nano (< 60):*
- Left: SHELL
- Center: active window name, truncated to fit
- Right: HH:MM

*tiny (60 – 79):*
- Left: SHELL
- Center: active window name
- Right: HH:MM

*small (80 – 119):*
- Left: SHELL │ session name (abbreviated)
- Center: active window name (1 window shown)
- Right: VCS dirty indicator (symbol only, no branch name) │ HH:MM

*medium (120 – 159):*
- Left: SHELL │ session name
- Center: window list (active + up to 1 neighbor each side)
- Right: VCS branch + dirty status │ HH:MM │ MM/DD

*large (160 – 199):*
- Left: SHELL │ hostname │ session name
- Center: window list (active + up to 2 neighbors each side)
- Right: VCS branch + dirty status │ uptime │ HH:MM │ MM/DD

*xlarge (200 – 239):*
- Left: SHELL │ user@hostname │ session name
- Center: full window list (all windows, collapse only if still insufficient space)
- Right: VCS branch + dirty status │ battery%* │ uptime │ HH:MM │ MM/DD

*xxlarge (≥ 240):*
- Left: SHELL │ user@hostname │ session name │ working directory (abbreviated with ~)
- Center: full window list
- Right: VCS branch + dirty status │ memory% │ load avg │ weather* │ battery%* │ uptime │ HH:MM │ MM/DD

*(\* omitted silently if unavailable)*

**Window list collapse rules (center section):**
- The center section is always visible and never fully hidden
- The active window is always shown; it is never dropped
- When space does not allow showing all windows, add the nearest left neighbor first, then the nearest right neighbor, alternating outward until space is exhausted
- Example: windows [dev, prod, database, server, logs], active = database, space for 2 → [prod, database]; space for 3 → [prod, database, server]; space for 4 → [dev, prod, database, server]
- When only one window fits, show only the active window with no truncation indicator needed (the count is implicit)

**Conditional display rules:**
- Users must be able to attach display conditions to any component using width comparisons, e.g. `show: columns >= 240` to show weather only on xxlarge terminals
- Conditions are evaluated at render time; a component that fails its condition is omitted exactly as if it were unavailable hardware

### Zen mode
- Must support a distraction-free zen mode that hides the status bar and all chrome, showing only the active pane at full size

### Bell handling
- Must support both audible bell (system audio) and visual bell (brief screen flash); each must be independently enable/disable-able
- Must support routing a bell event to a desktop notification — a pane or window that rings its bell while not focused must be able to trigger a notification
- Bell behavior must be configurable per pane, per window, and globally, with the most specific setting taking precedence
- All bell handling can be fully disabled; must default to visual bell only (no audio) so casterm never makes unexpected noise without user opt-in

### Mouse support
- Must support click-to-focus panes
- Must support drag-to-resize pane borders
- Must support mouse scroll within panes
- Must support double-click word selection and triple-click line selection
- Must support middle-click paste (Linux primary selection)
- Must support drag-and-drop pane reordering within and between windows in GUI mode
- Mouse support must be fully toggleable at runtime

### Text selection and paste
- Must support column (rectangular) selection mode — the selection is defined by two corner cells and copies only the characters within the selected columns, independent of line length
- Must support terminal hints — user-defined regex patterns that match content in the terminal output and can be activated by keyboard without entering copy mode or using the mouse; when the user invokes hint mode, all matches on screen are labeled and the user selects one with a key; supported actions include: open URL in browser, copy matched text, run an external command with the match as argument, and pipe the match to any built-in action; hint patterns must be user-configurable; the labeling alphabet must be user-configurable
- Must support quick select mode — a keyboard-driven overlay that highlights every on-screen instance of a common semantic element (URLs, IP addresses, file paths, git hashes, quoted strings) and lets the user jump to any match with a single keystroke without entering full copy mode; the recognized element types must be user-configurable
- Must support open-scrollback-in-editor — a keybinding that opens the scrollback buffer of the current pane in the user's `$EDITOR` as a static text file for searching and annotating; the pane continues running while the editor is open
- Must support open-scrollback-in-pager — open the scrollback buffer in the user's `$PAGER` with ANSI escape sequences preserved for color display
- Must support advanced paste — before delivering clipboard content to a pane, the user must be able to apply text transformations: convert tabs to spaces, strip trailing whitespace, prefix each line with a quoting character, remove leading indentation, strip ANSI escape sequences, base64 encode/decode; transformations must be accessible from the command palette and must be composable
- Must support slow paste — paste character-by-character with configurable delay between characters for terminals or applications that cannot handle rapid input
- Must support control character filtering on paste — optionally strip unusual control characters from pasted text
- Must support editable paste preview — when the unsafe paste warning appears, allow the user to edit the text before confirming
- Must support clipboard auto-copy on selection — optionally copy selected text to clipboard immediately when selection is made, without requiring an explicit copy command
- Must support granular PRIMARY vs CLIPBOARD configuration — independent control over which X11/Wayland selection buffers are used for select-to-copy and paste operations

### Activity and silence monitoring
- Must support per-pane and per-window monitoring for activity (new output) and silence (no output for a configurable period)
- Must visually indicate monitored panes/windows that have triggered their condition
- Must support sending a desktop notification or a status bar alert when a monitored condition triggers

### URL and content detection
- Must detect URLs, file paths, and other semantic content in terminal output and make them accessible (open in browser, copy, navigate to file) without mouse clicks in TUI mode and via click in GUI mode
- Must support Quick Open — Ctrl+click (or configurable modifier+click) on a filename opens it in the user's configured editor; if the filename includes a line number (e.g. `file.txt:42`), open at that line
- Must support URL open in background — optionally open hyperlinks without stealing focus from the terminal
- Must support registering protocol URL handlers — register `ssh://`, `telnet://`, `serial://` URL schemes so clicking links in other applications opens casterm
- Must support triggers — user-defined rules each consisting of a regex pattern and an action; when a pane produces output matching the pattern the action fires automatically without interrupting the pane; supported actions include: highlight the matched text with a configurable color, send a desktop notification with the match as context, run an external command with the match as an argument, and speak the matched text (text-to-speech for accessibility); triggers are passive and must never modify the pane's input or output stream
- Must support coprocess — a companion external program that runs alongside a specific pane; the coprocess receives a copy of all of that pane's output on its stdin, and anything the coprocess writes to its stdout is injected into the pane as if the user had typed it; coprocess attachment and detachment must not interrupt the pane; useful for automation, logging, and protocol adapters
- Must support annotations — the user must be able to attach a freeform text note to any line or range of lines in a pane's scrollback; annotations persist for the session lifetime; annotated lines must be visually marked when scrolled past; all annotations must be listable and navigable as a searchable collection
- Must support IDE-mode output capture — automatically detect structured output patterns common in development workflows (compiler errors with file and line, test failure summaries, linter warnings) and surface matches as a navigable list; the user must be able to jump forward and backward through captured entries without re-running a search
- Must support progress detection — passively detect when a running command emits a recognized progress bar or percentage-complete value and surface it as a system-level progress indicator (OS taskbar badge, notification) where the platform supports it; detection must never interfere with terminal output

### Session recording
- Must support recording a terminal session to a file for later replay
- Recorded sessions must be replayable within casterm and compatible with the asciinema format

### Lifecycle hooks
- Must support user-defined commands that run automatically on lifecycle events: session created, session destroyed, window created, window closed, pane exited, pane title changed
- Hooks must be opt-in and must never block the terminal when not configured

### Environment management
- Each session must carry its own environment variable snapshot taken at creation time
- New windows and panes must inherit environment from their parent session unless overridden
- Must support per-session environment variable overrides without affecting the host environment

### Scripting and layout files
- Must support user-authored layout definition files — a declarative file that fully describes a workspace: which windows and panes to create, what command to run in each pane, each pane's initial working directory, and which layout algorithm to apply; executing a layout file must produce the same result as manually performing all those operations interactively
- Layout files must be shareable and version-controllable; a project repository may include a layout file that any developer runs to spin up a standard development environment in one command
- Must support a remote control interface — a socket or named pipe that accepts the same commands available interactively; external programs and scripts must be able to query and drive a running casterm instance programmatically without attaching to a session; must support password authentication for secure IPC
- Must support running all session, window, and pane commands as CLI invocations against an already-running instance (e.g. `casterm new-window`, `casterm send-keys --pane main "make test"`) so shell scripts can drive casterm without opening an interactive session
- Must support structured state export — on request, return the complete current state (all sessions, windows, panes, working directories, running commands, sizes) as a machine-readable structured format so external tools can build on it
- Must support script recording — record keystrokes and automatically generate replayable scripts
- Must support hardcopy — instant dump of current window display to a file (separate from session logging)
- Must support wall message — broadcast a message to all attached displays
- Must support idle command execution — run any user-specified command after a configurable period of inactivity

### Built-in features (must work without installing external tools)
- TLDR simplified man pages (network access required; must degrade gracefully when offline)
- Cheat.sh cheat sheets (network access required; must degrade gracefully when offline)
- DevHints quick reference (network access required; must degrade gracefully when offline)
- Urban Dictionary lookup (network access required; must degrade gracefully when offline)
- Thesaurus lookup (network access required; must degrade gracefully when offline)
- Auto-tail log file watcher
- Pastebin upload — must support at minimum: dpaste, hastebin, ix.io, pastebin.com
- URL shortening — must support at minimum: is.gd, TinyURL, v.gd
- Copy/paste with clipboard history and primary selection (Linux X11/Wayland)
- Smart semantic selection: automatically expand a click to the most meaningful unit (URL, file path, IP address, git hash, quoted string, word)
- Global search across pane scrollback, all panes, windows, or file content

### Connectivity
- Must include a native SSH client with a saved connection manager — users must be able to create, name, organize, and open SSH connections without leaving casterm and without relying on an external `ssh` binary; the connection manager must support jump hosts, local and remote port forwarding, X11 forwarding, and SSH agent forwarding (including the system agent and Pageant on Windows)
- Must support proxy traversal — connections must be able to route through SOCKS4, SOCKS5, HTTP CONNECT, Telnet proxies, or a user-specified local proxy command; proxy configuration must be per-connection or global
- Must support IPv6 connections natively
- Must support serial port connections — connect to and interact with a device over a serial interface (RS-232, USB-serial adapters) directly from casterm; must support configurable baud rate, parity, data bits, stop bits, and flow control; must support a hex view mode for raw byte inspection; covers embedded systems, network devices, and hardware debugging
- Must support a Telnet client — connect to Telnet servers and legacy network devices directly from casterm without an external client; useful for routers, switches, and serial console servers
- Must support Zmodem (and Xmodem/Ymodem) file transfer over serial and SSH panes — send and receive files using these protocols in sessions where SCP or SFTP is unavailable
- Must support file transfer over the terminal connection itself — a built-in capability to push and pull files through the active terminal stream without requiring a separate channel, SSH session, or pre-installed agent on the remote host
- Must support exposing a running multiplexer over TLS/TCP — an opt-in network listener that allows remote clients to attach to sessions over the network using certificate-based authentication, without requiring SSH; must be disabled by default
- Must support a web client — an opt-in HTTPS interface that provides browser-based access to sessions; each connected browser client has its own independent cursor; authentication requires a bearer token; HTTPS is mandatory for all non-localhost connections; the web client must support multiplayer — multiple users connected simultaneously each with their own cursor, enabling pair programming and remote teaching; the web client is disabled by default

### AI integration
- Must support command suggestion, error explanation, and shell assistance via multiple AI backends
- AI is optional — no feature may depend on AI availability; casterm works fully without any AI configured
- AI backends must be auto-detected at runtime; if found, enable silently; if not found, disable silently with no error
- Must never send terminal content or commands to any service without explicit user opt-in per backend

**Supported AI backends (auto-detected in priority order):**

| Priority | Backend | Detection | API | Notes |
|----------|---------|-----------|-----|-------|
| 1 | Ollama (server) | Probe localhost:11434 | REST (OpenAI-compatible) | Local inference, preferred |
| 2 | Ollama (CLI) | `which ollama` | Subprocess | Fallback if server not running |
| 3 | LM Studio | Probe localhost:1234 | REST (OpenAI-compatible) | Local inference |
| 4 | LocalAI | Probe localhost:8080 | REST (OpenAI-compatible) | Local inference |
| 5 | llama.cpp server | Probe localhost:8080 | REST | Local inference |
| 6 | Claude Code | `which claude` | Subprocess | Anthropic CLI |
| 7 | GitHub Copilot CLI | `which gh` + `gh copilot` | Subprocess | GitHub CLI |
| 8 | OpenAI Codex CLI | `which openai` | Subprocess | OpenAI CLI |
| 9 | Anthropic Claude API | `$ANTHROPIC_API_KEY` set | REST | Cloud; explicit opt-in |
| 10 | OpenAI API | `$OPENAI_API_KEY` set | REST | Cloud; explicit opt-in |
| 11 | Google Gemini API | `$GOOGLE_API_KEY` set | REST | Cloud; explicit opt-in |

**Detection priority:**
1. Ollama — server first, then CLI; local inference, privacy-preserving, no auth required
2. Other local servers (LM Studio, LocalAI, llama.cpp) — local inference alternatives
3. CLI tools (claude, gh copilot, openai) — subprocess invocation, uses tool's own auth
4. Cloud APIs — only if env var is set AND user has explicitly enabled cloud AI in config

**Cloud AI safeguards:**
- Cloud backends are NEVER auto-enabled — presence of API key alone is insufficient
- User must set `ai_cloud_enabled: true` in config to allow any cloud backend
- Before first cloud request, display one-time confirmation: "Allow sending terminal context to {provider}? [y/N]"
- Config option `ai_cloud_providers: [openai, anthropic, google]` whitelists which cloud providers are permitted

**AI features:**
- Command suggestion — suggest completions based on partial input and shell history
- Error explanation — parse error output and explain in plain English
- Command generation — describe what you want, get a shell command
- Man page summary — quick summary of command usage
- Script explanation — explain what a script or pipeline does

### Project and VCS awareness
- Must automatically detect project type from directory contents: Rust, Node, Python, Go, Ruby, Java, .NET, PHP, Docker, Kubernetes, Terraform, Ansible
- Must provide named startup templates that open pre-configured window sets: ssh, docker, dev, node, build, productivity

### Font system
- Must ship with built-in Nerd Fonts that require no system font installation: Source Code Pro, JetBrains Mono, Hack, FiraCode, Iosevka
- May use system fonts when available but must never require them
- Must support both TrueType and bitmap fonts with fallback
- Must support explicit font fallback chains — user-configurable ordered list of fallback fonts for missing glyphs
- Must support programming ligatures when the active font provides them (e.g. FiraCode's `!=`, `->`, `=>` ligatures)
- Ligature rendering must be independently toggleable — a user must be able to disable ligatures without changing the font
- Must support programmatic box drawing — generate box drawing characters algorithmically for perfect alignment regardless of font
- Must support text baseline alignment adjustment — per-pixel or percentage control of vertical text positioning within cells
- Must support runtime font changes via OSC 710/711/712/713 escape sequences — applications can request font changes without user intervention

### Display
- Must support configurable window transparency in GUI mode
- Must support background blur — Gaussian blur behind terminal content, separate from transparency
- Must support background video — use video files as terminal background, not just static images
- Must support animated GIF backgrounds
- Must support configurable font size, line spacing, and letter spacing
- Must support font auto-scaling — automatically resize font when window is resized
- Must support pinch-to-zoom gesture for font size on touchpad/touchscreen
- GPU acceleration must be used when available and must degrade gracefully to software rendering when not; GPU is never a requirement
- Must support explicit GPU backend selection where platform permits (Vulkan, Metal, DirectX, OpenGL)
- Must support per-pane line timestamps — display the wall-clock time at which each line of terminal output was received; timestamps must be independently toggleable (default off), must never alter the pane's scrollback content, and must be dismissible without any output being lost
- Must provide a visual scrollbar — an overlay indicator showing the current viewport position within the scrollback buffer; the scrollbar must be independently togglable, must not consume character-cell space (overlay rendering only), and must appear on hover or persistently based on configuration
- Must support a badge — a translucent overlay label displayed inside the terminal content area (not the status bar) showing user-configurable text such as hostname, session name, or username; the badge must not capture input and must not obscure text in a way that makes it unreadable
- Must support background images in GUI mode — a static image or animated GIF rendered behind the terminal text layer at configurable opacity; distinct from window-level transparency which affects the compositor
- Must support a Quick Terminal / hotkey window in GUI mode — a terminal overlay accessible system-wide via a configurable global hotkey; the window slides in from a configurable screen edge (top, bottom, left, or right) and slides back out on a second press or focus loss; the terminal continues running between invocations; useful as a drop-down scratchpad accessible from any application
- Must support synchronized rendering — honor the DECSET 2026 synchronized-output terminal mode; when an application enables it, defer redraws until the output batch is complete to prevent visible screen tearing during rapid updates
- Must support Instant Replay — a terminal-level recording that captures the full state of a pane at regular intervals in addition to the normal scrollback, allowing the user to rewind and view output that was overwritten or cleared from the screen; the capture interval and maximum retained history size must be configurable; Instant Replay is independent of and complementary to the scrollback buffer
- Must support command output framing — each command's output can be wrapped in a discrete, collapsible visual frame that groups the prompt, the command, and all output together; frames can be independently scrolled, copied, searched, and collapsed to a summary line; this provides an IDE-like view of terminal history without changing how commands or shells work
- Must support cosmetic CRT / retro display effects in GUI mode — optional visual filters including phosphor glow, scanlines, screen curvature, and color bleed that simulate the appearance of vintage terminal hardware; all effects must be independently enable/disable-able and must have zero impact on terminal correctness or copy/paste behavior; effects are disabled by default
- Must support a scroll map / minimap — a zoomed-out view of terminal contents on the right side for quick navigation through scrollback
- Must support smart cursor color and minimum contrast — automatically adjust cursor visibility against background

### Theme system
- Must default to dark mode
- Must support dark, light, and auto (follow system preference) variants
- Must detect terminal background when running nested (TUI mode) — use `$COLORFGBG` environment variable, OSC 11 query, or heuristics to determine if the parent terminal has a dark or light background; adapt colors accordingly
- Must support `$TERMINAL_BACKGROUND` environment variable override — user can force dark/light detection
- Must adapt colors for readability on both dark and light backgrounds — bright variants for dark backgrounds, standard variants for light backgrounds (e.g., bright red vs standard red)
- Must never hardcode colors — all colors must be overridable via a theme
- Must ship with all themes from alacritty-theme (https://github.com/alacritty/alacritty-theme) compiled into the binary — no external downloads, no network access required
- If a theme name is invalid or not found, fall back to default (dracula)
- Themes should target WCAG AA contrast ratios where possible

**Theme configuration:**

```yaml
theme:
  mode: dark       # dark | light | auto (follows system preference)
  name: dracula    # built-in theme name (~300 available)
  file: ""         # empty for built-in themes
```

Custom theme file:
```yaml
theme:
  mode: auto
  name: custom
  file: themes/mytheme.yml  # relative to config dir or absolute path
```

- `mode`: dark, light, or auto (auto follows system dark/light preference and applies theme colors accordingly)
- `name`: any built-in theme name, or "custom" when using a file
- `file`: empty string for built-in themes; path to custom theme file (relative to config dir or absolute like `~/.config/casterm/themes/mytheme.toml`)
- Theme file format matches alacritty theme format for drop-in compatibility
- Default: `mode: dark`, `name: dracula`, `file: ""`

**Built-in theme catalog (from alacritty-theme):**

Theme resolution:
- **Dual-mode themes** (have both `_dark` and `_light` variants): `mode` selects the variant
  - `name: gruvbox` + `mode: dark` → uses `gruvbox_dark`
  - `name: gruvbox` + `mode: light` → uses `gruvbox_light`
  - `name: gruvbox` + `mode: auto` → detects system preference, picks variant
- **Single-mode themes** (no variants): `mode` is ignored; the theme's colors define its nature
  - `name: dracula` → always dark, regardless of `mode` setting
  - `name: acme` → always light, regardless of `mode` setting

| Category | Themes |
|----------|--------|
| **Dual-mode** (both _dark and _light) | ashes, ayu, enfocado, everforest, github, gruvbox, kimbie, one, papercolor, pencil, selenized, solarized |
| **Dark + light variant** | nord (has nord_light), tokyo_night (has tokyo_night_light) |
| **Single-mode dark** | afterglow, argonaut, baitong, bluish, catppuccin, challenger_deep, citylights, Cobalt2, cyber_punk_neon, dark_pastels, dark_pride, deep_space, doom_one, dracula, falcon, flat_remix, flexoki, ganbaru, gnome_terminal, google, gotham, gruvbox_material, hardhacker, hatsunemiku, hyper, inferno, iris, iterm, kitty, konsole_linux, linux, Mariana, material_theme, meliora, midnight_haze, monokai, nordic, oceanic_next, omni, oxocarbon, palenight, panda, rainbow, rigel, rose_pine, seashells, smoooooth, snazzy, spacegray, taerminal, tender, terminal_app, thelovelace, tomorrow_night, ubuntu, vesper, wombat, xterm, zenburn |
| **Single-mode light** | acme, alabaster, modus_operandi, noctis_lux, papertheme, tomorrow |

### Accessibility
- Must honor the `NO_COLOR` environment variable: when set to any non-empty value, all ANSI color and styling output is suppressed and casterm renders in plain monochrome; this applies to the status bar, UI chrome, and all built-in command output
- Must ship a high-contrast theme suitable for users with low vision
- Must render correctly across all color depth levels: true-color (24-bit), 256-color, 16-color, and no-color; each level must be detectable and selected automatically, with manual override available in configuration
- All interactive UI elements (command palette, pane borders, status bar mode indicator) must remain navigable without relying on color alone — shape, label, or position must convey state independently of color
- Must support bidirectional text rendering toggle — enable/disable BiDi display for Arabic, Hebrew, and other RTL scripts
- Must support screen reader compatibility — expose terminal content and state changes via platform accessibility APIs where available

### Key bindings
- Must support a configurable prefix key; the default is Ctrl+Space
- Must support a set of global key bindings that work without a prefix key, covering at minimum: help, command palette, global search, and zen mode toggle
- Must support vim-aware pane navigation so vim split navigation and pane navigation do not conflict
- All key bindings must be user-remappable
- Must support configurable mouse bindings — mouse events combined with modifiers can trigger actions
- Must support Meta/Alt key behavior configuration — whether Meta/Alt sends escape prefix vs sets eighth bit
- Must support Compose key / dead key input — multi-key sequences for special characters (e.g. Compose+e+' → é) independent of IME
- Must support Unicode input mode — a dedicated mode for entering Unicode characters by name or codepoint
- Must support digraph input — insert special characters via two-character sequences (vi-style)

#### Default key bindings

All bindings below use the default prefix key (Ctrl+Space). The notation `PREFIX` means press the prefix key first, release, then press the next key. Keys are case-sensitive where noted.

**Global bindings (no prefix required):**

| Key | Action |
|-----|--------|
| F1 | Show help / keybinding reference |
| Ctrl+P | Open command palette |
| Ctrl+F | Global search (all panes) |
| Ctrl+Z | Toggle zen mode |
| Ctrl+H / Ctrl+J / Ctrl+K / Ctrl+L | Navigate panes (vim-aware: passes through to vim when vim is active) |
| Shift+Left / Shift+Right | Previous / next window |
| Alt+1 … Alt+9 | Switch to window 1–9 |

**Session management (PREFIX + key):**

| Key | Action |
|-----|--------|
| d | Detach from session |
| $ | Rename current session |
| s | Show session picker / tree browser |
| ( | Switch to previous session |
| ) | Switch to next session |
| L | Switch to last (most recently used) session |

**Window management (PREFIX + key):**

| Key | Action |
|-----|--------|
| c | Create new window |
| , | Rename current window |
| n | Next window |
| p | Previous window |
| ` | Last (most recently used) window |
| w | Show window picker |
| & | Kill current window (with confirmation) |
| 0–9 | Switch to window 0–9 |
| ' | Prompt for window index and switch |
| . | Move window to a new index |
| f | Find window by name |

**Pane management (PREFIX + key):**

| Key | Action |
|-----|--------|
| \ | Split pane horizontally (new pane to the right) |
| / | Split pane vertically (new pane below) |
| x | Kill current pane (with confirmation) |
| z | Toggle pane zoom (fullscreen) |
| ! | Break pane out to its own window |
| q | Show pane numbers (press number to switch) |
| o | Cycle to next pane |
| ; | Switch to last active pane |
| { | Swap pane with previous |
| } | Swap pane with next |
| Space | Cycle through layouts (even, main-vertical, main-horizontal, grid) |
| Up / Down / Left / Right | Navigate to pane in direction |
| Ctrl+Up / Ctrl+Down / Ctrl+Left / Ctrl+Right | Resize pane in direction (5 cells) |
| Alt+Up / Alt+Down / Alt+Left / Alt+Right | Resize pane in direction (1 cell) |
| m | Toggle pane mark |
| M | Clear all pane marks |

**Pane resizing (PREFIX + key, repeatable without re-pressing prefix):**

| Key | Action |
|-----|--------|
| H | Resize pane left |
| J | Resize pane down |
| K | Resize pane up |
| L | Resize pane right |

**Copy mode (PREFIX + key to enter, then vim-style navigation):**

| Key | Action |
|-----|--------|
| [ | Enter copy mode |
| ] | Paste from buffer |
| = | List paste buffers and select |
| # | List paste buffers |
| - | Delete most recent paste buffer |

**Copy mode navigation (while in copy mode):**

| Key | Action |
|-----|--------|
| h / j / k / l | Move cursor left / down / up / right |
| w / b | Move forward / backward by word |
| 0 / $ | Move to start / end of line |
| g / G | Move to top / bottom of scrollback |
| Ctrl+U / Ctrl+D | Page up / down (half screen) |
| Ctrl+B / Ctrl+F | Page up / down (full screen) |
| / | Search forward |
| ? | Search backward |
| n / N | Next / previous search match |
| v | Begin selection |
| V | Begin line selection |
| Ctrl+V | Begin rectangular selection |
| y | Yank (copy) selection and exit copy mode |
| Enter | Yank selection and exit copy mode |
| Escape / q | Exit copy mode without copying |

**Broadcast mode (PREFIX + key):**

| Key | Action |
|-----|--------|
| B | Toggle broadcast mode (confirm before enabling) |
| Ctrl+B | Toggle broadcast to all panes in current window |

**Miscellaneous (PREFIX + key):**

| Key | Action |
|-----|--------|
| : | Enter command prompt |
| ? | Show keybinding help |
| t | Show clock |
| r | Reload configuration |
| ~ | Show messages (notifications history) |
| i | Display pane/window info |
| C | Customize / open settings |

**Mouse bindings (GUI mode defaults):**

| Action | Binding |
|--------|---------|
| Click on pane | Focus pane |
| Double-click | Select word |
| Triple-click | Select line |
| Drag on pane border | Resize pane |
| Middle-click | Paste primary selection (Linux) |
| Scroll wheel | Scroll pane (enters copy mode if at prompt) |
| Ctrl+Click on URL | Open URL in browser |
| Ctrl+Click on file path | Open file in editor |
| Shift+Click | Extend selection |
| Right-click | Context menu |

**Function key bindings (no prefix):**

| Key | Action |
|-----|--------|
| F2 | Create new window |
| F3 | Previous window |
| F4 | Next window |
| F5 | Reload configuration |
| F6 | Detach session |
| F7 | Enter copy mode |
| F8 | Rename current window |
| F11 | Toggle fullscreen (GUI mode) |
| F12 | Toggle dropdown mode (if configured) |

#### Default values reference

The following table lists all configurable options with their default values. These defaults are written to the generated configuration file on first run.

**Appearance:**

| Option | Default | Description |
|--------|---------|-------------|
| theme.mode | dark | Theme mode: dark, light, or auto (follow system) |
| theme.name | dracula | Built-in theme name (~300 available) or "custom" |
| theme.file | "" | Custom theme file path (empty for built-in) |
| font_family | Source Code Pro Nerd Font | Primary font |
| font_size | 14.0 | Font size in points |
| line_spacing | 1.2 | Line height multiplier |
| letter_spacing | 0.0 | Extra space between characters |
| cursor_shape | ibeam | Cursor style: ibeam, block, or underline |
| cursor_color | teal | Cursor color (theme color name or hex) |
| cursor_blink | true | Whether cursor blinks |
| cursor_blink_interval | 500 | Blink interval in milliseconds |
| background_opacity | 1.0 | Window opacity (0.0–1.0); 1.0 = fully opaque |
| background_blur | false | Enable background blur behind window |
| ligatures | true | Enable programming ligatures |
| bold_is_bright | false | Render bold text as bright color |
| dim_inactive_panes | true | Dim unfocused panes slightly |

**Terminal behavior:**

| Option | Default | Description |
|--------|---------|-------------|
| shell | (auto-detected) | Full path to shell binary |
| scrollback_lines | 10000 | Lines of scrollback history per pane |
| scrollback_compression | true | Compress old scrollback to save memory |
| alternate_screen | true | Enable alternate screen buffer |
| bracketed_paste | true | Enable bracketed paste mode |
| mouse | true | Enable mouse support |
| mouse_scroll_lines | 3 | Lines per scroll wheel event |
| confirm_close_with_processes | true | Warn before closing tab with running processes |
| confirm_large_paste | true | Warn before pasting > 5 lines or dangerous patterns |
| close_on_exit | auto | When shell exits: always, never, or auto (close on clean exit) |
| audible_bell | false | Play system bell sound |
| visual_bell | true | Flash screen on bell |
| bell_notification | true | Send desktop notification on bell in background pane |
| focus_follows_mouse | false | Focus pane under mouse cursor |
| copy_on_select | true | Copy selection to clipboard immediately |
| trim_trailing_whitespace | false | Strip trailing whitespace from copied text |

**Window and geometry:**

| Option | Default | Description |
|--------|---------|-------------|
| initial_columns | 120 | Default window width in columns |
| initial_rows | 35 | Default window height in rows |
| remember_window_position | true | Restore window position on launch |
| remember_window_size | true | Restore window size on launch |
| tab_bar_position | top | Tab bar location: top or bottom |
| tab_bar_style | normal | Tab style: normal, slim, or hidden |
| new_tab_position | end | Where new tabs appear: end or adjacent |
| tab_cycle_wrap | true | Wrap around when cycling past last tab |
| hide_tab_bar_single_tab | false | Hide tab bar when only one tab exists |

**Status bar:**

| Option | Default | Description |
|--------|---------|-------------|
| status_bar_position | bottom | Status bar location: top or bottom |
| status_bar_visible | true | Show status bar |
| status_bar_separator | │ | Character between status components |
| status_datetime_format | %H:%M | Time format (strftime syntax) |
| status_date_format | %m/%d | Date format (strftime syntax) |

**Multiplexer:**

| Option | Default | Description |
|--------|---------|-------------|
| prefix_key | Ctrl+Space | Prefix key for multiplexer commands |
| prefix_timeout | 2000 | Milliseconds before prefix mode auto-cancels |
| base_index | 0 | Starting index for windows (0 or 1) |
| pane_base_index | 0 | Starting index for panes (0 or 1) |
| renumber_windows | true | Renumber windows sequentially when one closes |
| automatic_rename | true | Auto-rename windows based on running command |
| aggressive_resize | true | Resize to smallest attached client per-window |
| pane_border_style | single | Pane border style: single, double, heavy, or none |
| pane_border_lines | rounded | Border corners: rounded or square |
| display_panes_time | 1000 | Milliseconds to show pane numbers |
| display_time | 4000 | Milliseconds to show messages |
| history_limit | 10000 | Scrollback lines (alias for scrollback_lines) |
| monitor_activity | false | Monitor panes for activity |
| monitor_silence | 0 | Alert after N seconds of silence (0 = disabled) |

**Session:**

| Option | Default | Description |
|--------|---------|-------------|
| attach_to_existing | true | Attach to existing session if same name exists |
| session_auto_save | true | Auto-save session state for resurrection |
| session_save_interval | 60 | Seconds between auto-saves |
| show_welcome_screen | true | Show session picker on startup |
| default_layout | (none) | Layout file to apply on new session |

**Copy mode:**

| Option | Default | Description |
|--------|---------|-------------|
| copy_mode_style | vi | Copy mode keybindings: vi or emacs |
| word_separators | " -_@./\\:;,!?()[]{}'\"`<>=&#%$^*+~|" | Characters that delimit words |

**Search:**

| Option | Default | Description |
|--------|---------|-------------|
| search_case_sensitive | false | Case-sensitive search by default |
| search_wrap | true | Wrap search to beginning/end |
| incremental_search | true | Search as you type |

**Hints:**

| Option | Default | Description |
|--------|---------|-------------|
| hint_alphabet | asdfghjkl | Characters used for hint labels |
| url_regex | (standard URL pattern) | Regex for URL detection |
| file_path_regex | (standard path pattern) | Regex for file path detection |

**Services:**

| Option | Default | Description |
|--------|---------|-------------|
| pastebin_provider | dpaste | Default paste service |
| url_shortener | isgd | Default URL shortener |
| ai_backend | (auto-detected) | Active AI backend; first available from detection order |
| ai_model | (auto-detected) | AI model; first available from backend |
| ai_local_enabled | (auto-detected) | Local AI enabled if backend responds |
| ai_cloud_enabled | false | Cloud AI (OpenAI/Anthropic/Google); must be explicitly enabled |
| ai_cloud_providers | [] | Permitted cloud providers when cloud AI enabled |

**SSH and connections:**

| Option | Default | Description |
|--------|---------|-------------|
| ssh_keepalive_interval | 60 | Seconds between SSH keepalives |
| ssh_connection_timeout | 30 | Seconds before connection timeout |
| ssh_reconnect_attempts | 3 | Number of reconnect attempts |
| ssh_reconnect_delay | 5 | Seconds between reconnect attempts |

**Serial:**

| Option | Default | Description |
|--------|---------|-------------|
| serial_baud_rate | 115200 | Default baud rate |
| serial_data_bits | 8 | Data bits (5, 6, 7, or 8) |
| serial_parity | none | Parity: none, odd, or even |
| serial_stop_bits | 1 | Stop bits (1 or 2) |
| serial_flow_control | none | Flow control: none, hardware, or software |

**Performance:**

| Option | Default | Description |
|--------|---------|-------------|
| gpu_acceleration | auto | GPU backend: auto, vulkan, metal, opengl, or software |
| max_fps | 60 | Maximum render frame rate |
| lazy_render | true | Skip unchanged frames |

**Dropdown mode:**

| Option | Default | Description |
|--------|---------|-------------|
| dropdown_hotkey | (none) | Global hotkey to toggle dropdown |
| dropdown_height | 50 | Dropdown height as percentage of screen |
| dropdown_width | 100 | Dropdown width as percentage of screen |
| dropdown_position | top | Screen edge: top, bottom, left, or right |
| dropdown_animation | slide | Animation style: slide, fade, or instant |
| dropdown_animation_duration | 200 | Animation duration in milliseconds |
| dropdown_auto_hide | true | Hide dropdown on focus loss |

**Web client (when enabled):**

| Option | Default | Description |
|--------|---------|-------------|
| web_server_enabled | false | Enable web client interface |
| web_server_address | 127.0.0.1 | Bind address |
| web_server_port | 59421 | Listen port |
| web_server_tls | true | Require TLS (HTTPS) |
| web_server_cert | (none) | Path to TLS certificate |
| web_server_key | (none) | Path to TLS private key |
| web_server_token | (auto-generated) | Bearer token for authentication; generated on first enable if empty |

**Logging:**

| Option | Default | Description |
|--------|---------|-------------|
| session_logging | false | Log session output to file |
| log_directory | ~/.local/share/casterm/logs | Directory for log files |
| log_filename_pattern | %Y%m%d-%H%M%S-{session}.log | Log filename template |
| log_timestamps | true | Prefix log lines with timestamps |
| log_strip_ansi | true | Remove ANSI escapes from logs |

**Updates:**

| Option | Default | Description |
|--------|---------|-------------|
| update_channel | stable | Release channel: stable, beta, or daily |
| check_for_updates | true | Check for updates on startup |
| auto_update | false | Apply updates automatically (never without confirmation) |

### Command palette and completion
- Must provide a command palette with fuzzy search across commands, panes, files, and built-in help
- Must provide tab completion for built-in commands with context-aware argument types (sessions, windows, panes, files, directories)

### Configuration
- Must never require configuration to be functional — zero-config first run is a hard requirement
- Must support project-local configuration that takes precedence over user-global configuration; the project-local file lives in the current working directory
- Must generate a complete default configuration file on first run when none exists, writing every available option into it — not just a minimal starter set
- The generated default configuration must include the resolved full path of the detected shell, so the user can see exactly which shell binary will be used
- Every option in the generated configuration must have a plain-English comment on its own dedicated line immediately above the option explaining: what the option does, what values it accepts, and what the default is — never appended inline to the end of the option line; a user who has never read any external documentation must be able to understand and change any setting by reading the file alone
- Option names must be self-explanatory plain English words and phrases; no single-letter keys, no numeric magic values, no abbreviations that require prior knowledge
- Options must be grouped into named sections that mirror how a user thinks about the terminal (e.g. appearance, behavior, multiplexer, shell, status bar, keybindings, services) — never a flat alphabetical dump
- When an option depends on something not present on the current system (e.g. a battery widget when no battery is detected, weather when no location is set, GPU acceleration when no GPU is available), the generated config must still include the option, commented out, with an explanation of what is required to activate it
- Status bar configuration must be expressed as a readable ordered list of named components with simple conditional syntax — not as a format string, escape sequences, or a domain-specific mini-language; a user must be able to add, remove, or reorder components without understanding any syntax beyond the config format itself
- Conditional display rules for status bar components must be written in plain readable syntax (e.g. `show_when: columns >= 160`) that a first-time user can understand without documentation
- Keybinding configuration must name modifiers and keys in the way a user would say them aloud (e.g. `Ctrl+B`, `Alt+Left`, `F1`) — never raw keycodes or opaque numeric identifiers
- The configuration file format is YAML; both `.yml` and `.yaml` extensions are valid; when casterm searches for a config file, `.yml` takes precedence over `.yaml`; if a `.yaml` file exists and no `.yml` file exists, casterm must automatically rename it to `.yml` on next startup
- Changes to the configuration file must take effect on reload without restarting the application
- Must never silently ignore an unrecognized option; an invalid or unrecognized key must be reported with the key name and line number

### Configuration profiles
- Must support named configuration profiles — multiple complete configuration sets (font, color scheme, shell, keybindings, startup layout, and environment) each stored under a user-defined name; the user must be able to create, edit, and switch between profiles at runtime from the command palette without restarting
- Must support automatic profile switching — casterm must be able to match the current hostname, username, or working directory path against user-defined patterns and automatically activate the corresponding profile when a match is found; matching must be re-evaluated each time any of those values changes (e.g. on `cd` or SSH connection)
- Must support saved connection bookmarks — named shortcuts for frequently accessed SSH hosts, serial ports, and local directories; bookmarks must be accessible from the welcome screen and the command palette
- Must support per-bookmark profile association — each saved connection may specify a distinct profile with its own colors/font for visual identification
- Must support custom icon and color label per bookmark/tab — user-assigned icon and color for quick identification
- Must support specifying the initial layout and profile via CLI flags at launch — e.g. `casterm --profile work --layout dev-env` must open casterm with the named profile active and the named layout file applied, without interactive prompts
- Must support configuration cloud sync — optionally sync settings across devices via a cloud service or self-hosted instance

### Performance
- Must start and display the first shell prompt in under one second on any supported platform
- Must sustain 60 FPS rendering under normal terminal workloads
- Must impose no perceptible input latency

### Safety and UX
- Must warn before pasting content that appears dangerous (destructive commands, shell injection patterns) or unusually large
- Must require explicit confirmation before enabling broadcast mode, with extra emphasis when SSH panes are among the targets
- Must never silently discard input or lose clipboard content

### Mosh awareness
- Must detect when running inside a Mosh session (via `MOSH_CONNECTION` environment variable or equivalent signal)
- When Mosh is detected, must treat it as equivalent to SSH for purposes of prefix key auto-shifting in nested sessions
- Must not depend on Mosh being installed; Mosh awareness is detection-only and must degrade silently when not in a Mosh session

### Self-update
- Must support a built-in self-update command that downloads and applies a new release
- Must support three update channels, selectable in configuration:
  - **stable** — follows tagged version releases; the default channel
  - **beta** — follows pre-release tags; opt-in only
  - **daily** (also called **devel**) — always the latest build from the main branch; opt-in only
- Must verify the integrity and authenticity of a downloaded binary before replacing the running one
- Must display a summary of changes (release notes or changelog excerpt) before applying an update
- Must support a check-only mode that reports whether an update is available without applying it
- Must never apply an update automatically without explicit user confirmation; background update checks are permitted but installation always requires user action
- Channel definitions align with the CI/CD release pipeline (tagged release → stable, pre-release tag → beta, main branch build → daily)

### Compatibility mode
- Must detect its own invocation name from `argv[0]` and enter the corresponding compatibility mode automatically — no flag required
- When invoked as `tmux` (via symlink, copy, or rename): must accept tmux CLI subcommands and arguments; must behave as a drop-in replacement so existing tmux scripts, aliases, and muscle memory continue to work
- When invoked as `zellij` (via symlink, copy, or rename): must accept Zellij CLI arguments and behave as a drop-in replacement
- When invoked as `byobu` (via symlink, copy, or rename): must enter Byobu-compatible behavior
- In any compatibility mode, casterm's full native feature set remains available; the mode changes only the CLI interface and default keybindings presented to the user — not what the application can do
- Compatibility mode is a best-effort layer covering the most commonly used commands and workflows; full bug-for-bug emulation of edge cases is not required
- Must document which commands and behaviors are covered by each compatibility mode

### Dropdown / hotkey mode
- Must support a Quake-style dropdown mode: a configurable global hotkey causes the casterm window to slide in from a screen edge (default: top of screen) as an overlay on top of all other windows, and slide back out when the hotkey is pressed again or focus leaves
- The dropdown height, width (as percentage of screen dimensions), screen edge, and slide animation duration must all be user-configurable
- Must support configurable animation style — slide, fade, or instant
- The global hotkey must register as a system-wide shortcut that fires even when another application has focus (GUI mode only; TUI mode has no system-wide hotkey facility and must skip this silently)
- The dropdown must open to the most recently active session and must not disturb running processes or disconnect sessions when hidden
- Must support "open on mouse monitor" vs pinned monitor — either appear on the monitor where the mouse cursor currently is, or always on a user-specified monitor
- Must support auto-hide on focus loss with configurable delay before hiding
- Must support multiple independent dropdown instances on different hotkeys, each with its own configuration
- Dropdown mode and standard window mode are mutually exclusive per-instance and are selected at startup via configuration

### Connection management
- Must maintain a built-in host directory: a persistent, searchable address book of saved connections organized into user-defined folders and groups; each entry stores at minimum the hostname/address, port, protocol, username, authentication method, and any per-connection setting overrides
- The host directory must be accessible from the command palette with a single action and must support keyboard-driven navigation without a mouse
- Must support per-connection profiles: each saved connection may override the color theme, background color, font, status bar behavior, or any other display setting independently of the global configuration; this enables immediate visual identification of production vs staging vs development environments
- Each tab, window, and session may be assigned a user-configurable color label for quick identification; the color appears in the tab bar, the session list, and the window border
- Must support automatic session reconnect: when a network connection drops, casterm must detect the disconnection and attempt to reconnect; the reconnect behavior (automatic vs prompt, retry interval, retry count) must be user-configurable per connection
- Must support a master password that encrypts all stored credentials, SSH keys, and sensitive connection parameters; the encrypted store is unlocked at startup; casterm must never write credentials or private keys to disk in plaintext
- Must support SSH jump host / bastion host chains: a connection may traverse one or more intermediate hosts before reaching the final destination; the full chain is configured within casterm without requiring manual `~/.ssh/config` editing
- Must support per-connection SSH key selection: each saved connection may specify which key to use, independent of the system SSH agent or global key selection
- Must support SSH agent forwarding on a per-connection basis
- Must support an SSH port forwarding manager: local, remote, and dynamic (SOCKS proxy) forwarding rules must be configurable, startable, and stoppable within casterm without leaving the active session; the manager must show which tunnels are active
- Must support hardware security key authentication for SSH connections: FIDO2/U2F keys, smart cards, and PKCS#11 tokens
- Must support Kerberos/GSSAPI authentication for enterprise single sign-on environments
- Must support SSH key generation — create RSA, ECDSA, Ed25519, and Ed448 key pairs directly within casterm without requiring external tools; must support exporting keys in OpenSSH, RFC 4716, and PuTTY (.ppk) formats
- Must support SSH connection sharing/multiplexing — multiple sessions to the same host may share a single underlying SSH connection to reduce overhead and authentication prompts
- Must support session anti-idle/keepalive — prevent disconnections via SSH protocol keepalives, custom keepalive sequences, or periodic NOOPs; configurable per-connection
- Must support an integrated SFTP file browser: when connected to a remote host via SSH, a side panel may show the remote filesystem; files may be uploaded and downloaded by drag-and-drop in GUI mode or by keyboard command in TUI mode; the SFTP browser must not interrupt the active terminal session
- Must support SSH X11 forwarding: remote GUI applications may be displayed on the local display when X11 or XWayland is available

### Protocol support
- Must support SSH (version 2 minimum) as a first-class built-in protocol; for GUI-mode connections casterm must not require the system `ssh` binary, though it may delegate to it in TUI mode
- Must support user-configurable SSH cipher and algorithm selection — expose key exchange algorithms, ciphers, MACs, and compression options in the UI; allow reordering and disabling of algorithms; warn when weak algorithms are selected
- Must support Telnet connections
- Must support Rlogin connections (legacy unencrypted remote login)
- Must support serial port / RS-232 connections: the user selects a device path (e.g. `/dev/ttyUSB0` on Linux, `COM3` on Windows) and configures baud rate, data bits, parity, stop bits, and flow control; a terminal session opens over the serial link without any additional tools; this covers embedded development, hardware debugging, and network device configuration
- Must support serial port newline conversion — configurable CR/LF/CRLF mode per connection
- Must support automatic serial port reconnection — detect device disconnect/reconnect and re-establish the session automatically
- Must support sending TTY break signal over serial connections
- Must support raw TCP socket connections (netcat-style)
- Must support Mosh as a first-class protocol alongside SSH: casterm handles the local Mosh client side; the Mosh binary must be present on the remote host but not necessarily on the local machine
- Must support RDP client — connect to Windows Remote Desktop sessions in a tab without external tools
- Must support VNC client — graphical remote access to VNC servers in a tab
- Must support Kermit file transfer protocol in addition to Zmodem/Xmodem/Ymodem

### Session logging
- Must support logging the raw text output of any pane or session to a file; logging is independently toggleable per pane and may be auto-enabled for all new sessions via configuration
- Log files must contain plain text only — no ANSI escape sequences, no binary data — readable in any text editor without preprocessing
- Log entries may optionally be prefixed with a timestamp; timestamping is a per-session configuration option
- Must support appending to an existing log file or creating a new file per session; the filename pattern for auto-created log files must be user-configurable and may include tokens for session name, date, remote hostname, and similar
- Must support automatic log rotation — create new log files daily or by size with configurable naming patterns
- Must support timestamp injection after inactivity — automatically insert a timestamp line into the log after a configurable period of no output
- Must support SSH packet-level logging for debugging — log raw SSH protocol packets with optional password omission
- Must support log replay — play back recorded log files as if they were live terminal output
- Session logging is distinct from session recording (the asciinema-format feature): logging produces a human-readable permanent text archive; recording produces a timed screencast for playback

### Output triggers
- Must support user-defined output triggers: regular expressions evaluated against terminal output as text arrives in a pane
- When a trigger fires, the following actions must be available (the user configures which actions fire per trigger; multiple actions may fire simultaneously):
  - Highlight the matched text with a user-specified color (persistent keyword highlighting)
  - Show a desktop or status bar notification containing the matched text or a user-defined message
  - Run a shell command with the matched text available as an environment variable
  - Activate the bell for the pane
  - Switch focus to the pane where the match occurred
  - Write a timestamped annotation to the pane's session log
- Triggers must be configurable at global scope, per connection profile, per session, or per window; more specific scopes take precedence
- Triggers must be opt-in and must impose no perceptible overhead when none are active

### Shell integration
- Must support shell integration marks — visual markers at each command prompt that are navigable via keyboard shortcuts (jump to previous/next prompt)
- Must support mark coloring based on exit code — failed commands display a different colored mark
- Must support autocomplete from scrollback history — suggest words that have previously appeared in terminal output
- Must support transferring shell integration scripts to remote hosts automatically via SSH
- Must detect and integrate with zoxide (smart cd) — if zoxide is available, casterm's directory picker and jump-to-directory features should use zoxide's frecency database
- Must detect and integrate with fzf — if fzf is available, use it for interactive selection in command palette, file picker, and history search; fall back to built-in picker if fzf is absent
- Must detect and integrate with atuin — if atuin is available, use its history database for command suggestions; casterm's own history remains independent
- Must detect and integrate with starship — if starship is configured, defer prompt rendering to starship when in compatibility mode; in native mode, casterm renders its own status bar independently
- Must support command-not-found handling — when a command is not found, optionally suggest similar commands, offer to search package repositories, or invoke a user-configured handler script
- Must support stty passthrough configuration — allow disabling XON/XOFF flow control (stty stop/start undef) and other terminal settings that conflict with common keybindings (Ctrl+S, Ctrl+Q)

### Coprocesses
- Must support coprocesses: a bidirectional pipe between a running terminal session and a user-specified external program; output from the pane is streamed to the coprocess stdin, and the coprocess stdout is injected back into the pane as if typed; this enables expect-style automation, interactive scripting, and protocol bridging without modifying the running shell

### Macro recording
- Must support macro recording: the user starts a recording, performs keystrokes and commands in the terminal, then stops; the recorded sequence can be replayed by name or keybinding at any later time
- Recorded macros must support configurable per-keystroke delay and wait-for-output pauses so replay is compatible with programs that need time to respond
- Macros must be saved persistently by name and must survive restarts
- Macro replay must support a broadcast mode where the sequence is sent simultaneously to all panes in a broadcast group
- Must support writing macros as text files with control flow (conditionals, loops, wait-for-pattern, variable substitution); the syntax must be documented and approachable without prior programming experience

### Snippet library
- Must maintain a persistent named snippet library: a collection of frequently-used commands, scripts, or text fragments stored by name and reusable across sessions
- Each snippet must have a name, an optional description, and text content
- Snippets must support two invocation modes: insert (paste the text into the active pane without a newline) and execute (send the text followed by a newline)
- Snippets must support placeholder variables: tokens in the snippet text that prompt the user for a value at invocation time
- Snippets must be importable and exportable for sharing between machines or team members
- The snippet library must be accessible from the command palette, a keyboard shortcut, and the quick command toolbar

### Quick command toolbar
- Must support a user-configurable quick command toolbar: a persistent strip of labeled buttons, each mapped to a snippet, macro, built-in action, or shell command; the toolbar may be shown or hidden and is intended for operations too frequent to type yet too context-specific for a dedicated keybinding
- The toolbar position (top, bottom, side panel) and button appearance must be configurable
- Toolbar buttons must support icons, text labels, or both
- Must support a Quick Commands sidebar panel — a persistent side panel of saved one-click commands (distinct from the toolbar)

### Command composer
- Must support a command window / composer — a separate text area for composing multi-line commands with history and editing before sending to the terminal; useful for complex commands that are awkward to edit inline

### Portable mode
- Must support a portable configuration mode: when casterm detects its own configuration file in the same directory as the binary (rather than in the user home or system config locations), it uses that directory as the root for all configuration, session state, snippets, host directory, and logs
- Portable mode requires no flag or environment variable — the presence of a config file alongside the binary is the signal
- Portable mode enables a complete casterm setup to be carried on a USB drive or committed to a project directory and used without any installation on the host system

### Window behavior
- Must support always-on-top mode — keep terminal window above all other windows
- Must support minimize to system tray — minimize to system tray instead of taskbar
- Must support close window on exit behavior — configurable: always close, never close, or close only on clean exit (exit code 0)
- Must support DBus interface on Linux for desktop environment integration — expose session/window/pane control over DBus for shell extensions and scripts

### Line discipline and terminal modes
- Must support line discipline settings — local echo (on/off/auto), local line editing (on/off/auto), implicit CR in LF, implicit LF in CR
- Must support login shell vs non-login shell distinction — explicit control over whether shells are invoked as login shells
- Must support separate iconName vs window title — distinct strings for window title bar vs icon/taskbar label via OSC 0/1/2
- Must support scrollbar position configuration — left or right side
- Must support read-only pane/tab mode — lock a pane to prevent accidental input while still viewing output
- Must support runtime character encoding selection — change encoding (UTF-8, ISO-8859-*, legacy CJK) for current session without restart
- Must support encoding transcoding — convert between legacy encodings and UTF-8 on the fly for applications that don't support UTF-8

### Printing
- Must support printing terminal contents — send current visible content or scrollback to a printer; supports VT320-style print escape sequences

### Secret handling
- Must support secret redaction when sharing — automatically mask sensitive content (passwords, tokens) when sharing or logging terminal output

### Markers
- Must support programmatic markers — create navigable visual markers in terminal output that persist and can be jumped to by keyboard

### C1 control codes
- Must support configurable C1 control code handling — allow the user to choose whether to interpret 8-bit C1 codes (0x80–0x9F) as control codes or display them as printable characters (ISO-8859 range); default must be safe (interpret as control)

### Security and escape filtering
- Must support per-escape-sequence security policy (allowWindowOps-style) — fine-grained control over which escape sequences are allowed; dangerous sequences (window title changes, icon changes, OSC 52 clipboard access) must be independently allowable or blockable
- Must support a secure/restricted mode — disable potentially dangerous escape sequences (clipboard access, window resize, font changes) for untrusted sessions or when connected to untrusted hosts

### Startup and exit
- Must support run command at startup — execute a user-specified command automatically when a new session or window is created
- Must support stay-open mode — keep the terminal window open after the shell exits, displaying final output until explicitly closed
- Must support close protection — warn before closing when specific protected processes (user-configured list) are still running

### Tab completion and hints
- Must support inline command hints — display ghosted suggested completions based on command history as the user types (fish-style autosuggestion); must not require any shell plugin or shell integration script

### Undo
- Must support undo close tab — restore a recently closed tab, including its scrollback and state, within a configurable time window

### Split persistence
- Must preserve split layouts across sessions — when a session is resurrected, restore not just windows but the exact pane split layout within each window

### Environment variables
- Must support environment variable inheritance control — fine-grained control over which host environment variables are passed to child processes; allow, deny, and override lists

### Layout management
- Must support layout auto-save — persist the current window/pane layout automatically at configurable intervals; when casterm exits normally the layout state is up-to-date without explicit save
- Must support named layouts — user may define, save, and switch between multiple named layouts within a session; layouts are distinct from startup templates (which create new sessions)
- Must support layout stack — switch to a new layout while preserving the previous; pop back to restore; enables quick context switches without losing pane arrangements

### Configuration toggle patterns
- Status bar segments must be togglable with a simple boolean or presence/absence in a list; segments prefixed with `#` in a list are disabled (comment-style toggling pattern, human-readable and easy to enable/disable without deleting)
- Network-dependent features (weather, external lookups) must auto-disable without error when offline and auto-enable when connectivity returns
- Hardware-dependent features (battery, fan speed, GPU temp) must auto-disable when hardware is absent and auto-enable when hardware is detected

### Geometry and positioning
- Must support default window geometry — configurable initial width and height (in columns×rows or pixels) for new GUI windows
- Must support saved window positions — remember and restore the window position (x, y on screen) when reopening casterm
- Must support multi-monitor awareness — remember which monitor a window was on and restore to that monitor if still attached

### Tab behavior
- Must support tab cycling wrap — when navigating past the last tab, wrap to the first (and vice versa); must be configurable
- Must support new tab placement — new tabs can be added at end or adjacent to the active tab; configurable
- Must support slim/compact tabs mode — reduced tab bar height for maximizing terminal space

### Scrollback configuration
- Must support separate scrollback limits for different pane types — SSH sessions may have a different history limit than local shells
- Must support scrollback compression — compress older scrollback lines in memory to reduce RAM usage on long-running sessions

### Mouse wheel and scroll
- Must support configurable mouse scroll speed — number of lines per scroll event; integer, configurable
- Must support scroll key passthrough — let applications receive scroll events when they request mouse reporting

### Geometry saving
- Must support saving current geometry to config — a command or keybinding that writes the current window size and position back to the config file as defaults

### Serial port configuration
- Must support named serial port presets — save baud rate, parity, data bits, stop bits, and flow control as a named preset for reuse

### Startup templates from directory
- Must support auto-discovering startup templates from a directory — casterm scans a config subdirectory for layout/template files and lists them in the welcome screen without requiring explicit registration

### Session attach behavior
- Must support attach-to-existing session by default — when launching casterm, if a session with the same name already exists, attach to it instead of creating a new one; must be configurable (always attach / always new / ask)

### Mode switching
- Must support mode display timeout — after a mode is entered (prefix, copy, etc.), display the mode indicator for a configurable duration then auto-exit if no further input (prevents getting stuck in a mode)
- Must support locked mode — a mode where all casterm keybindings are disabled except a single escape sequence to exit; useful when running TUI apps that conflict with casterm bindings

### Web interface configuration (when web client is enabled)
- Must support configurable web server bind address and port
- Must support optional TLS with user-supplied certificate and key paths
- Must support disabling localhost-only restriction when explicitly configured (default is localhost-only for security)

### Force-close behavior
- Must support configurable behavior on SIGTERM/SIGINT/SIGQUIT/SIGHUP — detach (default), kill all sessions, or prompt user if possible

### UI hints
- Must support hiding startup tips and release notes — configurable flags to disable welcome tips and what's-new notices

### Pane frames
- Must support pane border frames — draw a visible border around each pane for clarity; must be independently toggleable
- Must support rounded corners on pane frames — when frames are enabled, optionally use rounded corner box drawing characters
- Must support hiding session name in pane frame title — show only pane title, not session prefix

### Stacked pane resize
- Must support stacked resize mode — when resizing in a direction where panes are stacked, grow/shrink the stack proportionally rather than only affecting the immediate neighbor

### Auto-layout
- Must support auto-layout on pane close — automatically rebalance remaining panes when one is closed; configurable (on/off)

### Scrollback editor
- Must support configurable scrollback editor — user specifies the command used when opening scrollback in an external editor (default: `$EDITOR` or `vim`)

### Styled underlines
- Must support styled underlines — curly, dotted, dashed, and double underline styles when the terminal and font support them; useful for LSP integration and semantic highlighting
