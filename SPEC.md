# CASTERM Project-Specific Rule Overrides (SPEC.md)

This file contains implementation rules that OVERRIDE the template (AI.md) or global conventions. SPEC.md > AI.md > global CLAUDE.md.

---

## Web Client Implementation

### Token Authentication

**Auto-generation behavior:**
- On first enable of web server (config `web_server_enabled: true`), if `web_server_token` is empty or unset, generate a cryptographically secure 32-byte token
- Token format: Base64url-encoded 32 random bytes (43 characters, no padding)
- Generation: `rand::rngs::OsRng` + `base64::engine::general_purpose::URL_SAFE_NO_PAD`
- Token stored in config file (`~/.config/casterm/custom.yml`) with restrictive permissions (0600)
- Token displayed once to user via stdout on first generation: `Web access token: {token}` — never logged to any file

**Authentication flow:**
- Bearer token in `Authorization` header: `Authorization: Bearer {token}`
- Query parameter fallback for WebSocket upgrade: `?token={token}` (WebSocket cannot send custom headers on initial handshake)
- Token validation: constant-time comparison (`subtle::ConstantTimeEq`)
- Failed auth: HTTP 401 with `WWW-Authenticate: Bearer realm="casterm"`, no body (enumeration mitigation)
- Successful auth: session cookie set (HttpOnly, Secure, SameSite=Strict, 24h expiry) to avoid re-sending token on every request

**Token regeneration:**
- CLI command: `casterm web --regenerate-token`
- Invalidates all existing sessions immediately
- Displays new token once to stdout

### TLS Requirements

**Non-localhost connections:**
- TLS required — plain HTTP rejected with 421 Misdirected Request and message: "TLS required for remote access"
- Minimum TLS 1.2 (RFC 8446 compliance)
- Certificate sources (checked in order):
  1. User-provided cert/key in config: `web_server_cert`, `web_server_key`
  2. ACME/Let's Encrypt auto-provisioning if `web_server_domain` is set and port 443 is available
  3. Self-signed certificate generated on first enable (with loud warning to stdout)

**Localhost connections:**
- Plain HTTP allowed (token still required)
- `127.0.0.1` and `::1` only — not `0.0.0.0` or any other interface

### Web UI Design

**Dark-first, design token system:**
- All colors from ui_ux_conventions.md design token system — no hardcoded hex values
- Default theme: dark (`data-theme="dark"` on `<html>`)
- Theme toggle: client-side JS updates `data-theme` and persists to localStorage
- `@media (prefers-color-scheme: ...)` fallback for no-JS

**Server-side rendering:**
- Go `html/template` for all HTML generation
- Every page works without JavaScript (progressive enhancement)
- No client-side routing, no SPA, no React/Vue/Angular
- Terminal grid rendered as semantic HTML table with ARIA roles

**Mobile-first layout:**
- Base CSS for mobile; `@media (min-width: ...)` for tablet/desktop
- Breakpoints from ui_ux_conventions.md: 768px (tablet), 1024px (desktop), 1440px (wide)
- Touch targets minimum 44x44px
- Viewport meta tag: `<meta name="viewport" content="width=device-width, initial-scale=1">`

**Terminal rendering:**
- Canvas-based terminal grid for performance (>1000 cells/frame)
- Fallback: HTML table with `<span>` per cell for browsers without Canvas support
- Font: embedded monospace (same as native GUI — Source Code Pro Nerd Font)
- Font loading: `@font-face` with `font-display: swap`
- Cursor: CSS animation (ibeam shape, teal color per IDEA.md defaults)

**WebSocket protocol:**
- Endpoint: `/ws` (or `/ws?token={token}` for initial auth)
- Binary frames for terminal I/O (raw bytes, not JSON)
- Text frames for control messages (JSON):
  ```json
  {"type": "resize", "cols": 120, "rows": 40}
  {"type": "ping"}
  {"type": "focus", "pane": "pane-uuid"}
  ```
- Ping/pong every 30s for keepalive
- Reconnect with exponential backoff: 1s, 2s, 4s, 8s, max 30s

### Multiplayer Cursor Implementation

**Cursor visibility:**
- Each connected client has a unique cursor color (generated from client ID hash)
- Cursor displayed as colored block with username label on hover
- Cursor positions broadcast to all clients viewing the same session
- Rate limit: cursor position updates max 10/sec per client

**Protocol:**
```json
{"type": "cursor", "pane": "uuid", "row": 10, "col": 25, "user": "alice"}
```

**Privacy:**
- Cursor sharing opt-in per session (config or runtime toggle)
- When disabled, only own cursor visible
- User names from config `web_client_name` or auto-generated "User-XXXX"

### Status Bar (Web)

- Same segments as native (from IDEA.md)
- Rendered as flex container with left/center/right sections
- Mode indicator always far left
- Responsive: segments collapse to icons on narrow viewports
- Update via WebSocket push, not polling

### Keyboard Handling

- All key bindings from IDEA.md work identically in web client
- Prefix key (default Ctrl+Space) captured before browser gets it
- Browser shortcuts (Ctrl+T, Ctrl+W, etc.) NOT intercepted — user expects browser behavior
- `keydown` event handling with `e.preventDefault()` for terminal-bound keys only

### Accessibility (Web Client)

- WCAG 2.1 AA compliance
- Semantic HTML: `<main>`, `<nav>`, `<aside>` landmarks
- Terminal grid: `role="grid"`, `role="row"`, `role="gridcell"`
- Focus management: focus trapped in modal overlays, returned to trigger on close
- Screen reader: announce mode changes, bell notifications, errors
- Skip link: "Skip to terminal" as first focusable element
- High contrast: respect `@media (prefers-contrast: more)`

### Error States

- Connection lost: banner at top "Connection lost — reconnecting..." with retry countdown
- Auth failed: redirect to token entry page (no session cookie = show form)
- Session not found: 404 page with link to session list
- Never `alert()` — toast notifications for transient errors, modal for blocking errors

### Performance

- Initial page load < 500ms (measured at localhost)
- WebSocket latency < 50ms for keystroke → echo on localhost
- Canvas rendering at 60fps when terminal content changes
- Gzip compression for HTML/CSS/JS assets
- No external CDN dependencies — all assets embedded in binary

---

## Rule Overrides

### Web-specific override: client-side JS allowed for terminal

The global rule "no business logic in JS" is overridden for the web terminal client:
- Terminal grid rendering (Canvas API)
- Keyboard event capture and forwarding
- WebSocket message handling
- Cursor position tracking

These are I/O concerns, not business logic. All validation, authentication, and session management remain server-side.

### Token storage: exception to "never store tokens in plaintext"

The web access token is stored in plaintext in the config file (`~/.config/casterm/custom.yml`). This is acceptable because:
- The config file has 0600 permissions (owner-only read/write)
- The token grants local-equivalent access — if an attacker can read the config, they already have local access
- Hashing the token would require storing the hash AND the salt AND re-prompting the user on every restart

The token is NEVER logged, NEVER included in error messages, and NEVER transmitted except over TLS (or localhost).
