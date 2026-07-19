#!/bin/sh
# CASTERM container entrypoint
# Remaps UID/GID to match host volume ownership, then execs the app.
set -e

# If CASTERM_UID/CASTERM_GID are set, remap the casterm user
if [ -n "${CASTERM_UID}" ] && [ "$(id -u)" = "0" ]; then
    CASTERM_GID="${CASTERM_GID:-${CASTERM_UID}}"
    groupmod -o -g "${CASTERM_GID}" casterm 2>/dev/null || true
    usermod -o -u "${CASTERM_UID}" casterm 2>/dev/null || true
    chown -R casterm:casterm /home/casterm 2>/dev/null || true
    exec gosu casterm "$@"
fi

exec "$@"
