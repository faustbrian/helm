#!/usr/bin/env bash

set -euo pipefail

if [[ "$#" -ne 2 ]]; then
    printf 'usage: %s <published-image@sha256:digest> <evidence-directory>\n' \
        "$0" >&2
    exit 64
fi

readonly reference="$1"
readonly evidence_directory="$2"
readonly extension_source="images/php/8.5/extensions.txt"

if [[ ! "$reference" =~ @sha256:[0-9a-f]{64}$ ]]; then
    printf 'runtime acceptance requires an exact sha256 image reference\n' >&2
    exit 64
fi

if [[ ! -f "$extension_source" ]]; then
    printf 'runtime extension catalog is missing: %s\n' "$extension_source" >&2
    exit 66
fi

extensions=()
while IFS= read -r extension; do
    if [[ ! "$extension" =~ ^[a-z0-9_-]+$ ]]; then
        printf 'runtime extension catalog contains invalid name: %s\n' \
            "$extension" >&2
        exit 65
    fi
    extensions+=("$extension")
done < "$extension_source"

if [[ "${#extensions[@]}" -eq 0 ]]; then
    printf 'runtime extension catalog must not be empty\n' >&2
    exit 65
fi

workspace="$(mktemp -d)"
readonly workspace
readonly amd64_tag="stackctl-php-runtime-acceptance:amd64"
readonly arm64_tag="stackctl-php-runtime-acceptance:arm64"

cleanup() {
    docker image rm "$amd64_tag" "$arm64_tag" >/dev/null 2>&1 || true
    rm -rf "$workspace"
}
trap cleanup EXIT

cp "$extension_source" "$workspace/extensions.txt"
printf '%s\n' \
    "FROM $reference" \
    'COPY extensions.txt /tmp/stackctl-php-extensions.txt' \
    'RUN xargs docker-php-ext-enable < /tmp/stackctl-php-extensions.txt' \
    > "$workspace/Dockerfile"

mkdir -p "$evidence_directory"

readonly verify_program="foreach (array_slice(\$argv, 1) as \$extension) { if (!extension_loaded(\$extension)) { fwrite(STDERR, 'missing PHP extension: ' . \$extension . PHP_EOL); exit(1); } }"

for architecture in amd64 arm64; do
    platform="linux/$architecture"
    if [[ "$architecture" == "amd64" ]]; then
        tag="$amd64_tag"
    else
        tag="$arm64_tag"
    fi

    docker buildx build \
        --platform "$platform" \
        --load \
        --tag "$tag" \
        "$workspace"

    {
        printf 'source_reference=%s\n' "$reference"
        printf 'platform=%s\n' "$platform"
        docker image inspect "$tag" \
            --format 'image_id={{.Id}} os={{.Os}} architecture={{.Architecture}}'
        docker run --rm --platform "$platform" "$tag" php --version
        docker run --rm --platform "$platform" "$tag" \
            php -r "$verify_program" "${extensions[@]}"
        printf 'all supported extensions loaded: %s\n' "${extensions[*]}"
    } | tee "$evidence_directory/extensions-$architecture.txt"
done
