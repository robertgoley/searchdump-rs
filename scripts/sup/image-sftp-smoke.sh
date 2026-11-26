#!/usr/bin/env bash
# L-831 (1.3.1): sftp:// smoke test of a built image (the OpenSSH client inside the image, not the host's).
#   scripts/sup/image-sftp-smoke.sh <image> [port]
# Starts the L-809a test sshd (scripts/opendal-tests/containers/openssh.Containerfile) on 127.0.0.1,
# then runs the image as its own non-root user with --network=host: local -> sftp, sftp -> local,
# and a searchdump-multi tree copy to sftp. Exit 0 when every document comes back.
set -euo pipefail
IMAGE=${1:?usage: image-sftp-smoke.sh <image> [port]}
PORT=${2:-19924}
CONTAINER=${CONTAINER:-podman}
HERE=$(cd "$(dirname "$0")/.." && pwd)
W=$(mktemp -d); NAME=edrs-img-sftp-$$
# files the image's user wrote are owned by a subordinate uid under rootless podman: remove them in its namespace
cleanup() { $CONTAINER rm -f "$NAME" >/dev/null 2>&1 || true; $CONTAINER unshare rm -rf "$W" 2>/dev/null || rm -rf "$W" 2>/dev/null || true; }
trap cleanup EXIT
$CONTAINER build --network=host -q -t localhost/edrs-odt-openssh:latest \
  -f "$HERE/opendal-tests/containers/openssh.Containerfile" "$HERE/opendal-tests/containers" >/dev/null
mkdir -p "$W/keys" "$W/data"
ssh-keygen -q -t ed25519 -N '' -C edrs-img -f "$W/keys/id_ed25519"
cp "$W/keys/id_ed25519.pub" "$W/keys/odt"
chmod 755 "$W/keys"; chmod 644 "$W/keys/odt" "$W/keys/id_ed25519"   # owner differs from the image user
chmod 777 "$W/data"
$CONTAINER run -d --name "$NAME" --network=host -v "$W/keys/odt:/etc/ssh/keys/odt:ro,Z" localhost/edrs-odt-openssh:latest \
  sh -c "ssh-keygen -A >/dev/null && exec /usr/sbin/sshd -D -e -p $PORT -o ListenAddress=127.0.0.1 \
    -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no \
    -o AuthorizedKeysFile=/etc/ssh/keys/%u -o 'Subsystem sftp /usr/lib/ssh/sftp-server'" >/dev/null
for _ in $(seq 1 30); do (exec 3<>/dev/tcp/127.0.0.1/"$PORT") 2>/dev/null && break; sleep 1; done
python3 - "$W/data/in.json" <<'PY'
import json, sys
with open(sys.argv[1], "w") as f:
    for i in range(500):
        f.write(json.dumps({"_index": "odt", "_id": str(i), "_source": {"n": i, "t": f"doc {i} \u00e9"}}, ensure_ascii=False) + "\n")
PY
printf 'key_file = /keys/id_ed25519\nknown_hosts = accept\n' >"$W/data/sftp.ini"; chmod 644 "$W/data/sftp.ini"
URL=sftp://odt@127.0.0.1:$PORT/home/odt
run() { $CONTAINER run --rm --network=host -e HOME=/data -v "$W/keys/id_ed25519:/keys/id_ed25519:ro,Z" -v "$W/data:/data:Z" "$@"; }
rc=0
run "$IMAGE" --quiet --outputStorageConfig /data/sftp.ini --input /data/in.json --output "$URL/plain.json" || rc=1
run "$IMAGE" --quiet --inputStorageConfig /data/sftp.ini --input "$URL/plain.json" --output /data/back.json || rc=1
if [ $rc = 0 ] && cmp -s <(sort "$W/data/in.json") <(sort "$W/data/back.json"); then echo "ok: $IMAGE sftp round trip (500 docs)"; else echo "FAIL: $IMAGE sftp round trip"; rc=1; fi
mkdir -p "$W/data/tree"; cp "$W/data/in.json" "$W/data/tree/odt.data.json"; echo '{"version":1}' >"$W/data/tree/multielasticdump.json"; chmod -R a+rwX "$W/data/tree"
if run --entrypoint /usr/local/bin/searchdump-multi "$IMAGE" --direction copy --outputStorageConfig /data/sftp.ini --input /data/tree --output "$URL/tree" \
   && $CONTAINER exec "$NAME" test -s /home/odt/tree/odt.data.json; then echo "ok: $IMAGE searchdump-multi copy to sftp"; else echo "FAIL: $IMAGE searchdump-multi copy to sftp"; rc=1; fi
run --entrypoint /usr/bin/ssh "$IMAGE" -V 2>&1 | sed "s|^|ssh in $IMAGE: |"
exit $rc
