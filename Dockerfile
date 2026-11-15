# syntax=docker/dockerfile:1.7
#
# searchdump container image. scripts/build-image.sh (or the Makefile) picks the variant.
#
# Default (L-810c, owner decision 2026-11-11): dhi.io Debian 13, target `runtime-shell`, amd64,
# tags <ver> and latest; glibc + mimalloc (the default for *-linux-gnu builds, see Cargo.toml)
#   builder: dhi.io/rust:1.99.1-debian13-dev
#   runtime: dhi.io/debian-base:trixie-debian13-dev (shell, apt; uid 65532; OpenSSH client for sftp://)
# A plain `podman build .` builds this default (runtime-shell is the last stage).
#
# Other build types, same Dockerfile through the ARGs below (pins in the Makefile):
#   dhi-static  dhi.io/rust:1.99.1-debian13-dev -> dhi.io/static:20250419-glibc-debian13, target
#               `runtime` (glibc, libgcc_s, CA bundle, tzdata, uid 65532; no shell and no package
#               manager, so that stage is COPY-only). Tag <ver>-static. The default before L-810c.
#   dhi-alpine  dhi.io/rust:1.99.0-alpine3.23-dev -> dhi.io/alpine-base:3.24-alpine3.24-dev, target
#               `runtime-shell` (bash, ca-certificates and libgcc added, packages upgraded at build
#               time; musl's own allocator). Tag <ver>-alpine.
#   alpine      rust:1.99.1-alpine3.24 -> alpine:3.24.3 (public, musl, not default). Tag <ver>-alpine-public.
#   public      rust:1.99-trixie -> debian:trixie-slim (public, glibc, not default). Tag <ver>-debian-public.
# Base comparison (size, CVEs, arch): results/local/L-808b.md, SECURITY-CVE.md.
#
# Every type ships the compatibility symlinks in /usr/local/bin (L-810c): `elasticdump` and
# `elasticdump-rs` -> searchdump, `multielasticdump` -> searchdump-multi, so commands written for the
# old names (`--entrypoint multielasticdump`, a k8s `command: [multielasticdump]`) keep working.
# Neither binary looks at argv[0]; searchdump-multi finds searchdump through the resolved executable
# path, so a symlinked name behaves exactly like the real one.
# Builder and runtime always use the same libc. dhi.io needs `podman login dhi.io` /
# `docker login dhi.io` (pull only; credentials never enter the image).
ARG BUILDER_IMAGE=dhi.io/rust
ARG BUILDER_TAG=1.99.1-debian13-dev@sha256:be6c6ec91c61ae15b80521052df1c7f60fefd303293563b3dbd9f57e58783809
ARG BASE_IMAGE=dhi.io/debian-base
ARG BASE_TAG=trixie-debian13-dev@sha256:94571f3dc07fbc7166f6f1cbf1fe068a7647198ee2554e6b90d59a68cc8b053c

# ----------------------------------------------------------------- builder
FROM ${BUILDER_IMAGE}:${BUILDER_TAG} AS builder
# Release profile = fat LTO + codegen-units=1: the link step is the memory
# peak (~1.5-2 GB). Keep the job count low on small hosts.
ARG CARGO_BUILD_JOBS=2
# CARGO_HOME pinned so the registry cache mount below works for every builder
# (official rust images already use it; Alpine-packaged cargo defaults to ~/.cargo).
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS} \
    CARGO_HOME=/usr/local/cargo \
    CARGO_NET_OFFLINE=true \
    CARGO_TERM_COLOR=never
WORKDIR /src
# Alpine builders (the Alpine variants) need libc headers
# and a CA bundle; Debian/DHI builders already have both.
RUN if command -v apk >/dev/null 2>&1; then apk add --no-cache musl-dev ca-certificates; fi
# Offline build (L-808i): every crate comes from .vendor/, fetched on the HOST by
# scripts/vendor-crates.sh (run by scripts/build-image.sh and the Makefile image targets).
# The builder needs no DNS or registry access; the cargo step runs with --network=none.
COPY .vendor ./.vendor
COPY Cargo.toml Cargo.lock ./
COPY src ./src
# Cargo.toml declares the test targets; their sources must exist to load it.
COPY tests ./tests
# Workspace members (test-only tools; not built into the image).
COPY tools ./tools
RUN --network=none --mount=type=cache,target=/src/target,id=edrs-target \
    test -f .vendor/config.toml \
      || { echo "no .vendor/: run scripts/vendor-crates.sh on the host first (or use scripts/build-image.sh / make image)" >&2; exit 1; } \
 && cargo build --locked --offline --release --config .vendor/config.toml --bin searchdump --bin searchdump-multi \
 && install -D -m 0755 target/release/searchdump /out/searchdump \
 && install -D -m 0755 target/release/searchdump-multi /out/searchdump-multi \
 && /out/searchdump --version && /out/searchdump-multi --version
# CA bundle for runtimes that lack one (runtime-shell copies it only when missing).
RUN install -D -m 0644 /etc/ssl/certs/ca-certificates.crt /out/ca-certificates.crt
# License and third-party notices, shipped in every image (L-808c).
COPY LICENSE THIRD-PARTY-NOTICES.md /out/root/usr/share/doc/searchdump-rs/
# Root tree for the COPY-only `runtime` stage (bases without a shell): the binaries, the
# compatibility symlinks (elasticdump, elasticdump-rs, multielasticdump) and an empty /data.
RUN install -D -m 0755 /out/searchdump /out/root/usr/local/bin/searchdump \
 && install -D -m 0755 /out/searchdump-multi /out/root/usr/local/bin/searchdump-multi \
 && ln -s searchdump /out/root/usr/local/bin/elasticdump \
 && ln -s searchdump /out/root/usr/local/bin/elasticdump-rs \
 && ln -s searchdump-multi /out/root/usr/local/bin/multielasticdump \
 && install -d -m 0755 /out/root/data

# ----------------------------------------------------------------- base (labels, env)
FROM ${BASE_IMAGE}:${BASE_TAG} AS base
ARG VERSION=1.0.0
ARG VCS_REF=unknown
ARG BUILD_DATE=unknown
ARG BASE_IMAGE
ARG BASE_TAG
LABEL org.opencontainers.image.title="searchdump-rs" \
      org.opencontainers.image.description="Back up, restore and copy Elasticsearch and OpenSearch data (elasticdump-compatible CLI, written in Rust)" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.source="https://github.com/robertgoley/searchdump-rs" \
      org.opencontainers.image.base.name="${BASE_IMAGE}:${BASE_TAG}"
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

# ----------------------------------------------------------------- runtime (dhi-static)
# Shell-less glibc base (dhi.io/static, build type dhi-static): no RUN, only COPY. The base already ships
# glibc, libgcc_s, the CA bundle and the 65532 nonroot user.
FROM base AS runtime
COPY --from=builder /out/root/usr/local/bin/ /usr/local/bin/
COPY --from=builder /out/root/usr/share/doc/searchdump-rs/ /usr/share/doc/searchdump-rs/
COPY --from=builder --chown=65532:65532 /out/root/data /data
USER 65532:65532
WORKDIR /data
ENTRYPOINT ["/usr/local/bin/searchdump"]
CMD ["--help"]

# ----------------------------------------------------------------- runtime-shell (default)
# Bases with a shell (dhi-debian = default, dhi-alpine, alpine, public): `--target runtime-shell`.
# Kept as the last stage so a plain `podman build .` gives the default image.
FROM base AS runtime-shell
COPY --from=builder /out/searchdump /usr/local/bin/searchdump
# Bulk dump/load of regex-selected indices (runs searchdump from the same directory).
COPY --from=builder /out/searchdump-multi /usr/local/bin/searchdump-multi
COPY --from=builder /out/root/usr/share/doc/searchdump-rs/ /usr/share/doc/searchdump-rs/
# - OpenSSH client (L-810i, owner decision 2026-11-14): OpenDAL's sftp:// backend runs the system
#   `ssh` binary (Dropbear's client is not compatible). Alpine: openssh-client-default (ssh only,
#   no keysign); Debian: openssh-client without recommends, apt lists removed, and the setuid bit
#   dropped from ssh-keysign (host-based auth only; the image runs as 65532 with no setuid files).
#   L-810j (owner decision 2026-11-14): the account and service tools its dependencies pull in
#   (passwd: useradd/usermod/chpasswd/vipw/...; init-system-helpers: service, update-rc.d,
#   invoke-rc.d, deb-systemd-*; adduser's example hook; their dpkg maintainer scripts) are
#   deleted. A non-root image never runs them, and test-image keeps its executable allowlist
#   unchanged. The packages stay in the dpkg database, so SBOMs and CVE scans still see them.
#   The COPY-only dhi-static type has no package manager and stays without SFTP.
# - compatibility symlinks: elasticdump, elasticdump-rs -> searchdump; multielasticdump -> searchdump-multi
# - CA bundle only when the base image has none
# - /data: writable working directory for dumps (mount a volume here)
RUN --mount=type=bind,from=builder,source=/out/ca-certificates.crt,target=/tmp/ca.crt \
    if command -v apk >/dev/null 2>&1; then \
      apk upgrade --no-cache && apk add --no-cache bash ca-certificates libgcc openssh-client-default; \
    elif command -v apt-get >/dev/null 2>&1; then \
      export DEBIAN_FRONTEND=noninteractive \
      && apt-get update \
      && apt-get install -y --no-install-recommends openssh-client \
      && apt-get clean \
      && rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*.deb /var/log/apt /var/log/dpkg.log \
      && if [ -e /usr/lib/openssh/ssh-keysign ]; then chmod u-s /usr/lib/openssh/ssh-keysign; fi \
      && rm -f /usr/bin/chage /usr/bin/expiry /usr/bin/deb-systemd-helper /usr/bin/deb-systemd-invoke \
           /usr/sbin/chgpasswd /usr/sbin/chpasswd /usr/sbin/groupadd /usr/sbin/groupdel /usr/sbin/groupmod \
           /usr/sbin/grpck /usr/sbin/grpconv /usr/sbin/grpunconv /usr/sbin/newusers /usr/sbin/pwck \
           /usr/sbin/pwconv /usr/sbin/pwunconv /usr/sbin/shadowconfig /usr/sbin/useradd /usr/sbin/userdel \
           /usr/sbin/usermod /usr/sbin/vipw /usr/sbin/vigr /usr/sbin/service /usr/sbin/update-rc.d \
           /usr/sbin/invoke-rc.d /usr/share/doc/adduser/examples/adduser.local \
           /var/lib/dpkg/info/init-system-helpers.postinst /var/lib/dpkg/info/login.defs.postinst \
           /var/lib/dpkg/info/passwd.preinst /var/lib/dpkg/info/passwd.postinst \
           /var/lib/dpkg/info/passwd.prerm /var/lib/dpkg/info/passwd.postrm; \
    fi \
 && ssh -V \
 && ln -s searchdump /usr/local/bin/elasticdump \
 && ln -s searchdump /usr/local/bin/elasticdump-rs \
 && ln -s searchdump-multi /usr/local/bin/multielasticdump \
 && if [ ! -s /etc/ssl/certs/ca-certificates.crt ]; then \
      mkdir -p /etc/ssl/certs && cp /tmp/ca.crt /etc/ssl/certs/ca-certificates.crt; \
    fi \
 && mkdir -p /data && chown 65532:65532 /data
# 65532 = "nonroot" in Docker Hardened Images (and distroless); numeric so it
# also works on bases without that passwd entry.
USER 65532:65532
WORKDIR /data
ENTRYPOINT ["/usr/local/bin/searchdump"]
CMD ["--help"]
