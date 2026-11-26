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
BASE_TAG      ?= trixie-debian13-dev@sha256:36cada6ceac36fc775a1ce0597dc7a02b4f97d48163d955a0b382f2a0ae6add0
# Docker Hardened Images, Debian 13 / glibc builder -> static glibc runtime (no shell; default before L-810c)
DHI_STATIC_BUILDER_IMAGE ?= dhi.io/rust
DHI_STATIC_BUILDER_TAG   ?= 1.99.1-debian13-dev@sha256:6d78c1696e4745d1e006193ddef94d354094c890a284fa8ad6b954a8777eb63d
DHI_STATIC_BASE_IMAGE    ?= dhi.io/static
DHI_STATIC_BASE_TAG      ?= 20250419-glibc-debian13@sha256:133e7708dab3dc8c84080758eb1b99ef63795ff39a8588d038db4d3c2abacfba
# Docker Hardened Images, Alpine/musl (the default before L-808b; 0 CVEs at L-808b)
DHI_ALPINE_BUILDER_IMAGE ?= dhi.io/rust
DHI_ALPINE_BUILDER_TAG   ?= 1.99.0-alpine3.23-dev@sha256:2d4387114840f672c75a8cec9414a300e07b285798382c76a466eafec4686b6e
DHI_ALPINE_BASE_IMAGE    ?= dhi.io/alpine-base
DHI_ALPINE_BASE_TAG      ?= 3.24-alpine3.24-dev@sha256:7e00fc0f6f9f84303dc2657aac557bdefe9877a66c388c0521c5794d51056efa
# Public fallback, not default (same Debian release, so the same glibc)
PUBLIC_BUILDER_IMAGE ?= docker.io/library/rust
PUBLIC_BUILDER_TAG   ?= 1.99-trixie@sha256:09027f873d45b8ecd1dc86f1fa13b7a62b875c961f017a0e331267b350430137
PUBLIC_BASE_IMAGE    ?= docker.io/library/debian
PUBLIC_BASE_TAG      ?= trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4
# Public musl fallback, not default (Alpine builder + Alpine runtime with bash)
ALPINE_BUILDER_IMAGE ?= docker.io/library/rust
ALPINE_BUILDER_TAG   ?= 1.99.1-alpine3.24@sha256:b1d5dd5a22a09404d3e798c0cf051b689077360374eb505fa5bb93c479693d87
ALPINE_BASE_IMAGE    ?= docker.io/library/alpine
ALPINE_BASE_TAG      ?= 3.24.3@sha256:be2656061b536c1883c4034790e21d0a09c5aaaf51968b89368a3c0371ea2c9a
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
