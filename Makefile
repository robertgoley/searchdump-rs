# Container image helpers. Nothing here pushes: tag and push yourself, e.g.
#   podman push $(REGISTRY)/$(IMAGE_NAME):$(IMAGE_TAG)
# scripts/build-image.sh builds every type (--type dhi-debian|dhi-static|dhi-alpine|public|alpine,
# --platform, --tag); the targets below run the same Dockerfile with the same pins.
# Every type has the compatibility symlinks elasticdump, elasticdump-rs, multielasticdump (L-810c).
#
#   make image                 # default (L-810c): dhi.io Debian 13 builder -> dhi.io/debian-base -dev runtime
#                              #   with a shell, glibc + mimalloc, linux/amd64 (:<ver>, :latest)
#   make image-dhi-debian      # the same as `make image`
#   make image-dhi-static      # dhi.io Debian 13 builder -> dhi.io/static glibc, no shell (:<ver>-static;
#                              #   the default before L-810c)
#   make image-dhi-alpine      # dhi.io Alpine/musl, musl allocator (:<ver>-alpine)
#   make image-public          # public Debian 13 bases, no login, not default (:<ver>-debian-public)
#   make image-alpine          # public Alpine/musl bases, no login, not default (:<ver>-alpine-public)
#   make vendor                # fetch crates on the host into .vendor/ (needs cargo; image targets run it)
#   make test-image            # SUP-017 for IMAGE_VARIANT: non-root, --read-only + /data, binary set, base digest
#   make sbom scan             # syft SBOM + grype/trivy scan of the IMAGE_VARIANT image
REGISTRY   ?= localhost
IMAGE_NAME ?= searchdump-rs
IMAGE_TAG  ?= $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
IMAGE      := $(REGISTRY)/$(IMAGE_NAME):$(IMAGE_TAG)

# Variant used by test-image/save/sbom/scan; selects docker/image-binaries.<variant>.txt and
# the tag suffix (dhi-debian | dhi-static | dhi-alpine | public | alpine)
IMAGE_VARIANT ?= dhi-debian
SUFFIX_dhi-debian :=
SUFFIX_dhi-static := -static
SUFFIX_dhi-alpine := -alpine
SUFFIX_public     := -debian-public
SUFFIX_alpine     := -alpine-public
# Full repo:tag for the built image (any image target): SEARCHDUMP_IMAGE=registry/repo:tag
# (environment or make variable). When set it replaces the default tags below.
# ELASTICDUMP_IMAGE is the deprecated name (before L-810a), still accepted.
SEARCHDUMP_IMAGE ?= $(ELASTICDUMP_IMAGE)
VARIANT_IMAGE := $(if $(SEARCHDUMP_IMAGE),$(SEARCHDUMP_IMAGE),$(IMAGE)$(SUFFIX_$(IMAGE_VARIANT)))
imgtags = $(if $(SEARCHDUMP_IMAGE),-t $(SEARCHDUMP_IMAGE),$(1))

CONTAINER ?= $(shell command -v podman >/dev/null 2>&1 && echo podman || echo docker)

# F-14: every base is pinned by digest (TAG = tag@sha256:...; multi-arch index digests).
# Refresh a digest with `podman pull <image>:<tag>` + `podman image inspect --format '{{.RepoDigests}}'`.
# Default (L-810c): Docker Hardened Images, Debian 13 / glibc builder -> Debian 13 -dev runtime
# (shell, apt). BUILDER_* is also the vendoring (scripts/vendor-crates.sh) and repro (scripts/sup/repro.sh) image.
BUILDER_IMAGE ?= dhi.io/rust
BUILDER_TAG   ?= 1.99.1-debian13-dev@sha256:6d78c1696e4745d1e006193ddef94d354094c890a284fa8ad6b954a8777eb63d
BASE_IMAGE    ?= dhi.io/debian-base
BASE_TAG      ?= trixie-debian13-dev@sha256:120051dda16c2611454d2c54a0959f23f80c32493d0e528a5d1348bca82dfbc8
# Docker Hardened Images, Debian 13 / glibc builder -> static glibc runtime (no shell; default before L-810c)
DHI_STATIC_BUILDER_IMAGE ?= dhi.io/rust
DHI_STATIC_BUILDER_TAG   ?= 1.99.1-debian13-dev@sha256:6d78c1696e4745d1e006193ddef94d354094c890a284fa8ad6b954a8777eb63d
DHI_STATIC_BASE_IMAGE    ?= dhi.io/static
DHI_STATIC_BASE_TAG      ?= 20250419-glibc-debian13@sha256:133e7708dab3dc8c84080758eb1b99ef63795ff39a8588d038db4d3c2abacfba
# Docker Hardened Images, Alpine/musl (the default before L-808b; 0 CVEs at L-808b)
DHI_ALPINE_BUILDER_IMAGE ?= dhi.io/rust
DHI_ALPINE_BUILDER_TAG   ?= 1.99.0-alpine3.23-dev@sha256:2d4387114840f672c75a8cec9414a300e07b285798382c76a466eafec4686b6e
DHI_ALPINE_BASE_IMAGE    ?= dhi.io/alpine-base
DHI_ALPINE_BASE_TAG      ?= 3.24-alpine3.24-dev@sha256:ab3130d401f94092d65c15d38566fbffdba1b4fe0f766968003535cdddf6d2e0
# Public fallback, not default (same Debian release, so the same glibc)
PUBLIC_BUILDER_IMAGE ?= docker.io/library/rust
PUBLIC_BUILDER_TAG   ?= 1.99-trixie@sha256:8a4e90c7946b74626488b1446f712c0fe0ed1a596cbd3d871684bc9c5433ae07
PUBLIC_BASE_IMAGE    ?= docker.io/library/debian
PUBLIC_BASE_TAG      ?= trixie-slim@sha256:8aa5a7bf5a1955bddbe7d74fcdd0b91ab30e74362a8b73af672b0481b35b44a0
# Public musl fallback, not default (Alpine builder + Alpine runtime with bash)
ALPINE_BUILDER_IMAGE ?= docker.io/library/rust
ALPINE_BUILDER_TAG   ?= 1.99.1-alpine3.24@sha256:44819b27be4c97b0e4d7203bd2e02c4820f97700211b6a37fb4c246a2de59383
ALPINE_BASE_IMAGE    ?= docker.io/library/alpine
ALPINE_BASE_TAG      ?= 3.24.4@sha256:f59abe656b692d901e184be3445e084786602108599d2a51d229c0f5a48c71bf
CARGO_BUILD_JOBS ?= 2
# Image platform for every image target (default linux/amd64, L-810c; linux/arm64 runs the build
# under emulation on an amd64 host).
PLATFORM ?= linux/amd64

VCS_REF    := $(shell git rev-parse --short HEAD 2>/dev/null || echo unknown)
BUILD_DATE := $(shell date -u +%Y-%m-%dT%H:%M:%SZ)

BUILD_ARGS = --build-arg VERSION=$(IMAGE_TAG) --build-arg VCS_REF=$(VCS_REF) \
             --build-arg BUILD_DATE=$(BUILD_DATE) --build-arg CARGO_BUILD_JOBS=$(CARGO_BUILD_JOBS)

.PHONY: help vendor image image-dhi-static image-dhi-alpine image-dhi-debian image-public image-alpine test-image save sbom scan print
help:
	@sed -n '1,19p' Makefile

print:
	@echo "IMAGE=$(IMAGE) VARIANT_IMAGE=$(VARIANT_IMAGE) CONTAINER=$(CONTAINER)"
	@echo "builder=$(BUILDER_IMAGE):$(BUILDER_TAG) base=$(BASE_IMAGE):$(BASE_TAG)"

# Crates are fetched on the host (needs cargo); the image build itself is offline (L-808i).
vendor:
	CONTAINER="$(CONTAINER)" scripts/vendor-crates.sh

# Same prerequisite checks as scripts/build-image.sh (container tool, host cargo, buildx, base pulls/login).
preflight = CONTAINER="$(CONTAINER)" scripts/build-image.sh --preflight $(1):$(2) --preflight $(3):$(4)

# default: dhi.io Debian 13 -dev runtime with a shell (`runtime-shell`), linux/amd64
image:
	@$(call preflight,$(BUILDER_IMAGE),$(BUILDER_TAG),$(BASE_IMAGE),$(BASE_TAG))
	CONTAINER="$(CONTAINER)" scripts/vendor-crates.sh
	$(CONTAINER) build $(BUILD_ARGS) --platform $(PLATFORM) --target runtime-shell \
	  --build-arg BUILDER_IMAGE=$(BUILDER_IMAGE) --build-arg BUILDER_TAG=$(BUILDER_TAG) \
	  --build-arg BASE_IMAGE=$(BASE_IMAGE) --build-arg BASE_TAG=$(BASE_TAG) \
	  $(call imgtags,-t $(IMAGE) -t $(REGISTRY)/$(IMAGE_NAME):latest) .

image-dhi-debian: image

# shell-less static base: the `runtime` stage (COPY only)
image-dhi-static:
	@$(call preflight,$(DHI_STATIC_BUILDER_IMAGE),$(DHI_STATIC_BUILDER_TAG),$(DHI_STATIC_BASE_IMAGE),$(DHI_STATIC_BASE_TAG))
	CONTAINER="$(CONTAINER)" scripts/vendor-crates.sh
	$(CONTAINER) build $(BUILD_ARGS) --platform $(PLATFORM) --target runtime \
	  --build-arg BUILDER_IMAGE=$(DHI_STATIC_BUILDER_IMAGE) --build-arg BUILDER_TAG=$(DHI_STATIC_BUILDER_TAG) \
	  --build-arg BASE_IMAGE=$(DHI_STATIC_BASE_IMAGE) --build-arg BASE_TAG=$(DHI_STATIC_BASE_TAG) \
	  $(call imgtags,-t $(IMAGE)$(SUFFIX_dhi-static)) .

image-dhi-alpine:
	@$(call preflight,$(DHI_ALPINE_BUILDER_IMAGE),$(DHI_ALPINE_BUILDER_TAG),$(DHI_ALPINE_BASE_IMAGE),$(DHI_ALPINE_BASE_TAG))
	CONTAINER="$(CONTAINER)" scripts/vendor-crates.sh
	$(CONTAINER) build $(BUILD_ARGS) --platform $(PLATFORM) --target runtime-shell \
	  --build-arg BUILDER_IMAGE=$(DHI_ALPINE_BUILDER_IMAGE) --build-arg BUILDER_TAG=$(DHI_ALPINE_BUILDER_TAG) \
	  --build-arg BASE_IMAGE=$(DHI_ALPINE_BASE_IMAGE) --build-arg BASE_TAG=$(DHI_ALPINE_BASE_TAG) \
	  $(call imgtags,-t $(IMAGE)$(SUFFIX_dhi-alpine)) .

image-public:
	@$(call preflight,$(PUBLIC_BUILDER_IMAGE),$(PUBLIC_BUILDER_TAG),$(PUBLIC_BASE_IMAGE),$(PUBLIC_BASE_TAG))
	CONTAINER="$(CONTAINER)" scripts/vendor-crates.sh
	$(CONTAINER) build $(BUILD_ARGS) --platform $(PLATFORM) --target runtime-shell \
	  --build-arg BUILDER_IMAGE=$(PUBLIC_BUILDER_IMAGE) --build-arg BUILDER_TAG=$(PUBLIC_BUILDER_TAG) \
	  --build-arg BASE_IMAGE=$(PUBLIC_BASE_IMAGE) --build-arg BASE_TAG=$(PUBLIC_BASE_TAG) \
	  $(call imgtags,-t $(IMAGE)$(SUFFIX_public)) .

image-alpine:
	@$(call preflight,$(ALPINE_BUILDER_IMAGE),$(ALPINE_BUILDER_TAG),$(ALPINE_BASE_IMAGE),$(ALPINE_BASE_TAG))
	CONTAINER="$(CONTAINER)" scripts/vendor-crates.sh
	$(CONTAINER) build $(BUILD_ARGS) --platform $(PLATFORM) --target runtime-shell \
	  --build-arg BUILDER_IMAGE=$(ALPINE_BUILDER_IMAGE) --build-arg BUILDER_TAG=$(ALPINE_BUILDER_TAG) \
	  --build-arg BASE_IMAGE=$(ALPINE_BASE_IMAGE) --build-arg BASE_TAG=$(ALPINE_BASE_TAG) \
	  $(call imgtags,-t $(IMAGE)$(SUFFIX_alpine)) .

test-image:
	CONTAINER=$(CONTAINER) scripts/test-image.sh $(VARIANT_IMAGE) $(IMAGE_VARIANT)

save:
	rm -f image.tar
	$(CONTAINER) save -o image.tar $(VARIANT_IMAGE)

sbom: save
	syft docker-archive:image.tar -o cyclonedx-json=sbom-image.cdx.json -o spdx-json=sbom-image.spdx.json

# Per-variant accepted-CVE files when they exist (L-810e: dhi-debian), else the shared grype file.
GRYPE_IGNORE ?= $(firstword $(wildcard docker/grype-ignore.$(IMAGE_VARIANT).yaml) docker/grype-ignore.yaml)
TRIVY_IGNORE ?= $(wildcard docker/trivyignore.$(IMAGE_VARIANT).yaml)

scan: save
	grype -c $(GRYPE_IGNORE) docker-archive:image.tar --fail-on high
	trivy image --input image.tar --severity HIGH,CRITICAL --exit-code 1 $(if $(TRIVY_IGNORE),--ignorefile $(TRIVY_IGNORE))
