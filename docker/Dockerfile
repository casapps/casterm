# CASTERM Runtime Image
# Two-stage: builder (casjaysdev/rust) + minimal Alpine runtime
# Build args set at build time by CI — no hardcoded values

ARG PROJECT_ORG=casapps
ARG PROJECT_NAME=casterm
ARG RUST_TARGET=x86_64-unknown-linux-musl

FROM casjaysdev/rust:latest AS builder

ARG PROJECT_ORG
ARG PROJECT_NAME
ARG RUST_TARGET

WORKDIR /build

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo ./.cargo
COPY src ./src
COPY build.rs ./
COPY assets ./assets

RUN cargo build --release --target "${RUST_TARGET}" \
    && cp "target/${RUST_TARGET}/release/${PROJECT_NAME}" "/usr/local/bin/${PROJECT_NAME}"

FROM alpine:latest

ARG PROJECT_NAME

RUN apk add --no-cache \
    ca-certificates \
    tini \
    gosu \
    shadow \
    && addgroup -g 1000 casterm \
    && adduser -D -u 1000 -G casterm -h /home/casterm casterm

COPY --from=builder "/usr/local/bin/${PROJECT_NAME}" "/usr/local/bin/${PROJECT_NAME}"
COPY docker/rootfs/ /

RUN chmod +x /usr/local/bin/entrypoint.sh

USER casterm
WORKDIR /home/casterm

STOPSIGNAL SIGTERM

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD pgrep -x casterm || exit 1

ENTRYPOINT ["tini", "-p", "SIGTERM", "--", "/usr/local/bin/entrypoint.sh"]
CMD ["casterm"]
