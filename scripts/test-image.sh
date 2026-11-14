#!/usr/bin/env bash
# shellcheck disable=SC2329  # check() invokes the helper functions indirectly
# Image hardening tests (TST-SUP-017, L-211, L-808b); `make test-image` runs this.
#   scripts/test-image.sh <image> [variant] [--update]
#     variant selects docker/image-binaries.<variant>.txt (default dhi-debian; --update rewrites it
#     after review). Images without /bin/sh (dhi-static) are inspected through `export`, not a shell.
# Checks:
#   1. --version / --help work; the entrypoint is searchdump; the compatibility symlinks
#      (elasticdump, elasticdump-rs -> searchdump, multielasticdump -> searchdump-multi) exist and each
#      runs as its target when used by bare name as the entrypoint (`--entrypoint multielasticdump`,
#      like a k8s `command: [multielasticdump]`); the CA bundle is present (plus bash on images
#      that have a shell)
#   2. it runs as non-root (65532), and the working dir is /data
#   3. read-only root filesystem: a file->file dump into a writable /data volume works under
#      --read-only --cap-drop ALL --security-opt no-new-privileges, and writing anywhere else fails
#   4. no executables beyond the documented set (docker/image-binaries.<variant>.txt)
#   5. the base image label is pinned by digest (org.opencontainers.image.base.name has @sha256:)
#   6. (L-810i) the OpenSSH client is present for sftp:// (not on dhi-static) and ssh-keysign is not setuid
# Image package SBOM diff: scripts/sup/sbom-pr.sh with IMAGE_SBOM_BASE/HEAD (SUP-011).
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
IMG="${1:?usage: test-image.sh <image> [variant] [--update]}"; VAR="${2:-dhi-debian}"
C="${CONTAINER:-podman}"; fail=0
ok() { echo "PASS $*"; }; bad() { echo "FAIL $*"; fail=1; }
check() { local msg="$1"; shift; if "$@"; then ok "$msg"; else bad "$msg"; fi; }
W=$(mktemp -d); trap 'rm -rf "$W"; $C rm -f edl-ti-c-$$ >/dev/null 2>&1; $C volume rm -f edl-ti-$$ >/dev/null 2>&1' EXIT

runs() { "$C" run --rm "$IMG" "$1" >/dev/null; }
check "--version" runs --version
check "--help" runs --help
ep=$($C image inspect "$IMG" --format '{{json .Config.Entrypoint}}')
check "entrypoint searchdump ($ep)" test "$ep" = '["/usr/local/bin/searchdump"]'

# Root filesystem listing without running anything inside the image.
if ! { $C create --name edl-ti-c-$$ "$IMG" >/dev/null && $C export edl-ti-c-$$ > "$W/rootfs.tar" \
       && tar -tvf "$W/rootfs.tar" > "$W/rootfs.lst"; }; then bad "export rootfs"; fi
$C rm -f edl-ti-c-$$ >/dev/null 2>&1
has() { awk -v p="$1" '$6 == p' "$W/rootfs.lst" | grep -q .; }
shell=0; has bin/sh && shell=1
ca_size=$(awk '$6 == "etc/ssl/certs/ca-certificates.crt" {print $3}' "$W/rootfs.lst")
check "searchdump-multi in /usr/local/bin" has usr/local/bin/searchdump-multi
# Compatibility symlinks (L-810c): present, pointing at the right binary, and runnable by bare name
# (PATH lookup, as a k8s `command:` or `--entrypoint NAME` does); the version line names the target.
ver=$($C run --rm "$IMG" --version 2>/dev/null | head -1); ver="${ver##* }"
for pair in elasticdump:searchdump elasticdump-rs:searchdump multielasticdump:searchdump-multi; do
  link="${pair%%:*}"; tgt="${pair#*:}"
  check "$link symlink -> $tgt" grep -qE " usr/local/bin/$link -> (/usr/local/bin/)?$tgt\$" "$W/rootfs.lst"
  out=$($C run --rm --entrypoint "$link" "$IMG" --version 2>&1 | head -1)
  check "--entrypoint $link --version runs $tgt ($out)" test "$out" = "$tgt $ver"
done
check "CA bundle present (${ca_size:-0} bytes)" test "${ca_size:-0}" -gt 0
check "THIRD-PARTY-NOTICES.md in /usr/share/doc/searchdump-rs" grep -qE ' usr/share/doc/searchdump-rs/THIRD-PARTY-NOTICES\.md$' "$W/rootfs.lst"
check "LICENSE in /usr/share/doc/searchdump-rs" grep -qE ' usr/share/doc/searchdump-rs/LICENSE$' "$W/rootfs.lst"
if [[ $shell == 1 ]]; then
  hasbash() { has bin/bash || has usr/bin/bash; }
  check "bash present (image has a shell)" hasbash
else
  ok "no shell in the image (no /bin/sh): shell checks skipped"
fi

# L-810i: sftp:// runs the system OpenSSH client; every image with a package manager ships it
# (the COPY-only dhi-static image does not), and ssh-keysign carries no setuid bit.
if [[ "$VAR" != dhi-static ]]; then
  check "OpenSSH client /usr/bin/ssh present (sftp://)" has usr/bin/ssh
fi
nosuid_keysign() { ! grep -qE '^-..[sS].* usr/lib/(openssh|ssh)/ssh-keysign$' "$W/rootfs.lst"; }
check "ssh-keysign not setuid" nosuid_keysign

user=$($C image inspect "$IMG" --format '{{.Config.User}}')
nonroot() { [[ -n "$1" && "${1%%:*}" != 0 && "${1%%:*}" != root ]]; }
check "non-root user in config ($user)" nonroot "$user"
if [[ $shell == 1 ]]; then
  u=$($C run --rm --entrypoint /bin/sh "$IMG" -c 'id -u')
  check "non-root at runtime (uid $u)" nonroot "$u"
fi
wd=$($C image inspect "$IMG" --format '{{.Config.WorkingDir}}')
check "workdir /data ($wd)" test "$wd" = /data

printf '{"_index":"i","_id":"1","_source":{"a":1}}\n{"_index":"i","_id":"2","_source":{"b":"x"}}\n' > "$W/in.ndjson"; chmod 644 "$W/in.ndjson"
$C volume create edl-ti-$$ >/dev/null
lines=""
if $C run --rm --read-only --cap-drop ALL --security-opt no-new-privileges --network none \
     -v "$W/in.ndjson:/in/in.ndjson:ro,Z" -v edl-ti-$$:/data "$IMG" --input /in/in.ndjson --output /data/out.ndjson >/dev/null 2>&1; then
  $C volume export edl-ti-$$ | tar -xOf - out.ndjson > "$W/out.ndjson" 2>/dev/null || true
  lines=$(wc -l < "$W/out.ndjson")
fi
check "read-only rootfs + writable /data volume: dump OK, ${lines:-0} lines (cap-drop ALL, no-new-privileges, no network)" test "${lines:-0}" = 2
# Writing outside /data must fail under --read-only (through the binary itself: works without a shell).
if $C run --rm --read-only --network none -v "$W/in.ndjson:/in/in.ndjson:ro,Z" "$IMG" \
     --input /in/in.ndjson --output /usr/local/bin/x.ndjson >/dev/null 2>&1; then
  bad "rootfs writable under --read-only"
else ok "rootfs not writable under --read-only"; fi

# Regular files with the owner-execute bit, as absolute paths (same format for every variant).
# (/etc/hosts, /etc/hostname, /etc/resolv.conf and /run/.containerenv are created by the container
# engine at `create`, not shipped in the image.)
awk '$1 ~ /^-..x/ {print "/" $6}' "$W/rootfs.lst" \
  | grep -vxE '/etc/(hosts|hostname|resolv\.conf)|/run/\.containerenv' | sort > "$W/bins.txt"
BL="docker/image-binaries.$VAR.txt"
if [[ "${3:-}" == --update ]]; then cp "$W/bins.txt" "$BL"; echo "updated $BL"; fi
# Shared-library file versions (libz.so.1.3.2 -> libz.so.1.4 after the runtime stage's
# `apk upgrade`) are compared without their version suffix: a new library is still flagged,
# a security update of a listed one is not.
sover() { sed -E 's/(\.so)(\.[0-9]+)+$/\1.*/' "$1" | sort -u; }
if [[ -f "$BL" ]]; then
  extra=$(comm -13 <(sover "$BL") <(sover "$W/bins.txt"))
  if [[ -z "$extra" ]]; then ok "executables within documented set ($(wc -l < "$W/bins.txt") files, $BL)"
  else bad "undocumented executables:"; while read -r l; do echo "  + $l"; done <<< "$extra"; fi
else bad "no baseline $BL (review, then rerun with: $0 $IMG $VAR --update)"; fi
base=$($C image inspect "$IMG" --format '{{index .Labels "org.opencontainers.image.base.name"}}')
pinned() { [[ "$1" == *@sha256:* ]]; }
check "base pinned by digest ($base)" pinned "$base"
exit $fail
