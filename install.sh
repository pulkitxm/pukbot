#!/bin/sh

set -eu
umask 077

REPOSITORY=pulkitxm/Pukbot
VERSION=${PUKBOT_VERSION:-latest}
BIN_DIR=${PUKBOT_INSTALL_DIR:-}

info() {
    printf 'info: %s\n' "$*"
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

has() {
    command -v "$1" >/dev/null 2>&1
}

usage() {
    printf '%s\n' \
        'Install Pukbot from its private GitHub Release.' \
        '' \
        'Usage: install.sh [OPTIONS]' \
        '' \
        'Options:' \
        '  -v, --version VERSION  Release such as v0.1.0 (default: latest)' \
        '  -b, --bin-dir DIR      Installation directory (default: ~/.local/bin)' \
        '  -h, --help             Print this help' \
        '' \
        'Environment variables:' \
        '  PUKBOT_VERSION          Same as --version' \
        '  PUKBOT_INSTALL_DIR      Same as --bin-dir'
}

require_value() {
    option=$1
    count=$2
    [ "${count}" -ge 2 ] || fail "${option} requires a value"
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        -v | --version)
            require_value "$1" "$#"
            VERSION=$2
            shift 2
            ;;
        --version=*)
            VERSION=${1#*=}
            shift
            ;;
        -b | --bin-dir)
            require_value "$1" "$#"
            BIN_DIR=$2
            shift 2
            ;;
        --bin-dir=*)
            BIN_DIR=${1#*=}
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            fail "unknown option: $1"
            ;;
    esac
done

has gh || fail "GitHub CLI is required"

if [ -z "${BIN_DIR}" ]; then
    if [ -n "${XDG_BIN_HOME:-}" ]; then
        BIN_DIR=${XDG_BIN_HOME}
    else
        [ -n "${HOME:-}" ] || fail "HOME is not set; provide --bin-dir"
        BIN_DIR=${HOME}/.local/bin
    fi
fi

case "${VERSION}" in
    latest)
        RELEASE_TAG=
        VERSION_LABEL=latest
        ;;
    '')
        fail "the release version cannot be empty"
        ;;
    *)
        case "${VERSION}" in
            v*) RELEASE_TAG=${VERSION} ;;
            *) RELEASE_TAG=v${VERSION} ;;
        esac
        case "${RELEASE_TAG}" in
            v[0-9]*) ;;
            *) fail "invalid release version: ${VERSION}" ;;
        esac
        case "${RELEASE_TAG}" in
            *[!0-9A-Za-z._+-]*) fail "invalid release version: ${VERSION}" ;;
            *) ;;
        esac
        VERSION_LABEL=${RELEASE_TAG}
        ;;
esac

OS=$(uname -s 2>/dev/null) || fail "could not identify the operating system"
ARCH=$(uname -m 2>/dev/null) || fail "could not identify the CPU architecture"

case "${OS}" in
    Darwin)
        case "${ARCH}" in
            x86_64 | amd64) ASSET=pukbot-macos-x86_64 ;;
            arm64 | aarch64) ASSET=pukbot-macos-aarch64 ;;
            *) fail "Pukbot does not publish a macOS release for architecture '${ARCH}'" ;;
        esac
        BINARY_NAME=pukbot
        ;;
    Linux)
        case "${ARCH}" in
            x86_64 | amd64) ASSET=pukbot-linux-x86_64 ;;
            arm64 | aarch64) ASSET=pukbot-linux-aarch64 ;;
            *) fail "Pukbot does not publish a Linux release for architecture '${ARCH}'" ;;
        esac
        BINARY_NAME=pukbot
        ;;
    MINGW* | MSYS* | CYGWIN*)
        case "${ARCH}" in
            x86_64 | amd64) ASSET=pukbot-windows-x86_64.exe ;;
            *) fail "Pukbot does not publish a Windows release for architecture '${ARCH}'" ;;
        esac
        BINARY_NAME=pukbot.exe
        ;;
    *)
        fail "unsupported operating system: ${OS}"
        ;;
esac

if has sha256sum; then
    CHECKSUM_TOOL=sha256sum
elif has shasum; then
    CHECKSUM_TOOL=shasum
else
    fail "sha256sum or shasum is required"
fi

TEMP_DIR=$(mktemp -d 2>/dev/null || mktemp -d -t pukbot) || fail "could not create a temporary directory"

cleanup() {
    rm -rf "${TEMP_DIR}"
}
trap cleanup EXIT HUP INT TERM

info "detected ${OS} ${ARCH}"
info "downloading Pukbot ${VERSION_LABEL}"

if [ -n "${RELEASE_TAG}" ]; then
    gh release download "${RELEASE_TAG}" \
        --repo "${REPOSITORY}" \
        --pattern "${ASSET}" \
        --pattern SHA256SUMS \
        --dir "${TEMP_DIR}" || fail "release download failed"
else
    gh release download \
        --repo "${REPOSITORY}" \
        --pattern "${ASSET}" \
        --pattern SHA256SUMS \
        --dir "${TEMP_DIR}" || fail "release download failed"
fi

EXPECTED_CHECKSUM=$(awk -v asset="${ASSET}" '
    {
        name = $NF
        sub(/^\*/, "", name)
        sub(/^dist\//, "", name)
        if (name == asset) {
            print $1
            exit
        }
    }
' "${TEMP_DIR}/SHA256SUMS")

case "${EXPECTED_CHECKSUM}" in
    '' | *[!0-9A-Fa-f]*) fail "the release checksum for ${ASSET} is invalid" ;;
    *) ;;
esac
[ "${#EXPECTED_CHECKSUM}" -eq 64 ] || fail "the release checksum for ${ASSET} is invalid"

case "${CHECKSUM_TOOL}" in
    sha256sum) ACTUAL_CHECKSUM=$(sha256sum "${TEMP_DIR}/${ASSET}" | awk '{print $1}') ;;
    shasum) ACTUAL_CHECKSUM=$(shasum -a 256 "${TEMP_DIR}/${ASSET}" | awk '{print $1}') ;;
    *) fail "no checksum tool is available" ;;
esac

[ "${EXPECTED_CHECKSUM}" = "${ACTUAL_CHECKSUM}" ] || fail "checksum verification failed"
info "verified SHA-256 checksum"

mkdir -p "${BIN_DIR}" || fail "could not create ${BIN_DIR}"
install -m 0755 "${TEMP_DIR}/${ASSET}" "${BIN_DIR}/${BINARY_NAME}" || fail "installation failed"
info "installed Pukbot to ${BIN_DIR}/${BINARY_NAME}"

case ":${PATH:-}:" in
    *:"${BIN_DIR}":*) ;;
    *) printf 'Add this directory to PATH: %s\n' "${BIN_DIR}" ;;
esac
