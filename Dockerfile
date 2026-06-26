# Thin wrapper over the official ntfy image: same server binary, plus the
# bundled backup/restore helpers the lifecycle tools shell out to. The base tag
# is pinned by build-arg so `ntfy.upgrade` and CI can move it deliberately.
ARG NTFY_VERSION=latest
FROM binwiederhier/ntfy:${NTFY_VERSION}

COPY scripts/backup.sh /usr/local/bin/backup
COPY scripts/restore.sh /usr/local/bin/restore
RUN chmod +x /usr/local/bin/backup /usr/local/bin/restore

EXPOSE 80

VOLUME ["/etc/ntfy", "/var/cache/ntfy"]

HEALTHCHECK --interval=30s --timeout=10s --start-period=20s --retries=3 \
    CMD wget -qO- http://localhost:80/v1/health > /dev/null || exit 1
