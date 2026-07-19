# CASTERM Docker Setup

## Building the Image

```bash
docker build \
  --build-arg PROJECT_ORG=casapps \
  --build-arg PROJECT_NAME=casterm \
  -t ghcr.io/casapps/casterm:latest \
  -f docker/Dockerfile .
```

## Running Tests

```bash
docker compose -f docker/docker-compose.test.yml run test
```

## Development

```bash
# Start a dev shell with the Rust toolchain
docker compose -f docker/docker-compose.dev.yml run dev

# Run the TUI
docker compose -f docker/docker-compose.dev.yml run dev cargo run

# Run with debug logging
RUST_LOG=debug docker compose -f docker/docker-compose.dev.yml run dev cargo run
```

## GUI with X11 Forwarding

```bash
# Grant X11 access (scoped to current user only)
xhost +SI:localuser:$(id -un)

# Run with X11 forwarding
docker compose -f docker/docker-compose.dev.yml run gui

# Revoke access when done
xhost -SI:localuser:$(id -un)
```

## GUI with Wayland Forwarding

```bash
docker compose -f docker/docker-compose.dev.yml run gui-wayland
```

## Cross-Compilation

All targets are handled by `casjaysdev/rust:latest`:

```bash
# Linux x86_64 (musl)
docker compose -f docker/docker-compose.dev.yml run dev \
  cargo build --release --target x86_64-unknown-linux-musl

# Linux aarch64 (musl)
docker compose -f docker/docker-compose.dev.yml run dev \
  cargo build --release --target aarch64-unknown-linux-musl
```

## Notes

- All `docker run` invocations use `--rm` and a named container (`casterm-XXXXXXXX`)
- The `docker-compose.test.yml` is the only compose file AI agents should use
- `docker-compose.yml` and `docker-compose.dev.yml` are human-only
