# Kubernetes examples

Job manifests that run the [dhi.io Debian image](../../README.md#container-image)
(`ghcr.io/robertgoley/searchdump-rs:1.3.0-dhi-debian`, published with `v1.3.0`; or a
locally loaded `localhost/searchdump-rs:1.3.0`). The image entrypoint is `searchdump`;
Jobs that need `searchdump-multi` or a short shell loop set `command` accordingly.
The image runs as uid 65532; every Job sets that user and mounts
[`pvc.yaml`](pvc.yaml) at `/data`.

These examples assume the default image, which has a shell. Do not switch them
to the shell-less `dhi-static` tag unless you rewrite the `command:` loops as
separate Jobs.

## Apply order

1. Create the dump PVC: `kubectl apply -f pvc.yaml` (size it for the dump; the
   file asks for 20Gi and leaves `storageClassName` unset so the cluster default
   is used).
2. Create the Secrets the Job you want actually refers to. The Job files
   **reference** Secret names and keys; they do not include values. Examples:

   ```bash
   # basic auth, used as environment variables (preferred over a URL password)
   kubectl create secret generic searchdump-es \
     --from-literal=username= \
     --from-literal=password=

   # optional --httpAuthFile (INI: user= / password=), chmod is not needed inside a Secret
   kubectl create secret generic searchdump-http-auth \
     --from-file=http-auth.ini=./http-auth.ini

   # TLS material for the secure-tls Jobs
   kubectl create secret generic searchdump-tls \
     --from-file=ca.pem=./ca.pem \
     --from-file=client.crt=./client.crt \
     --from-file=client.key=./client.key

   # OpenDAL INI for the object-storage Jobs (see examples/storage-config.ini.example)
   kubectl create secret generic searchdump-s3 \
     --from-file=storage.ini=./storage.ini
   ```

   Fill in the literal values or file contents yourself; never commit them.
3. Edit the Job's `ES_URL` / `INDEX` / `S3_URI` (and `SLICES` / `FILE_SIZE` as
   needed), then `kubectl apply -f <job>.yaml`.
4. Dump and restore Jobs share the PVC; do not run them at the same time
   (`ReadWriteOnce`). Delete a finished Job before re-applying the same file.

Set `imagePullSecrets` if the cluster cannot pull from GHCR (or load the image
into the node and set `imagePullPolicy: IfNotPresent`, already the default
here). For a private CA with `searchdump-multi`, also set `SSL_CERT_FILE` to
the mounted CA bundle; the multi binary has no `--ca`. See
[`../README.md`](../README.md).

A `CronJob` around any dump Job is a natural next step for a scheduled backup.
