#!/bin/sh
# Starts the daemon as the unprivileged `sovereign` user. Hosting platforms
# often mount volumes owned by root, so when started as root we first hand
# /data to that user, then drop root before the daemon runs.
set -e

if [ "$(id -u)" = "0" ]; then
    chown -R sovereign:sovereign /data || echo "warning: could not take ownership of /data" >&2
    exec setpriv --reuid=sovereign --regid=sovereign --init-groups \
        sovereign-daemon --data-dir /data "$@"
fi

exec sovereign-daemon --data-dir /data "$@"
