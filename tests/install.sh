#!/bin/sh
# shellcheck disable=SC2016,SC2310,SC2312

set -eu

ROOT=$(CDPATH='' cd -- "$(dirname "$0")/.." && pwd)
INSTALLER=${ROOT}/install.sh
TEST_ROOT=$(mktemp -d 2>/dev/null || mktemp -d -t gitbot-tests)
FIXTURES=${TEST_ROOT}/fixtures
FAKE_BIN=${TEST_ROOT}/bin
ORIGINAL_PATH=${PATH}

cleanup() {
    rm -rf "${TEST_ROOT}"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "${FIXTURES}" "${FAKE_BIN}"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

assert_contains() {
    needle=$1
    file=$2
    grep -F "${needle}" "${file}" >/dev/null 2>&1 || fail "expected '${needle}' in ${file}"
}

assert_equals() {
    expected=$1
    actual=$2
    [ "${expected}" = "${actual}" ] || fail "expected '${expected}', got '${actual}'"
}

sha256() {
    file=$1
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "${file}" | awk '{print $1}'
    else
        shasum -a 256 "${file}" | awk '{print $1}'
    fi
}

prepare_release() {
    asset=$1
    contents=$2
    printf '%s' "${contents}" >"${FIXTURES}/${asset}"
    printf '%s  %s\n' "$(sha256 "${FIXTURES}/${asset}")" "${asset}" >"${FIXTURES}/SHA256SUMS"
}

printf '%s\n' '#!/bin/sh' \
    'case "${1:-}" in' \
    '    -s) printf "%s\n" "$GITBOT_TEST_OS" ;;' \
    '    -m) printf "%s\n" "$GITBOT_TEST_ARCH" ;;' \
    '    *) exit 2 ;;' \
    'esac' >"${FAKE_BIN}/uname"

printf '%s\n' '#!/bin/sh' \
    'set -eu' \
    'destination=' \
    'write_url=false' \
    'url=' \
    'while [ "$#" -gt 0 ]; do' \
    '    case "$1" in' \
    '        -o) destination=$2; shift 2 ;;' \
    '        -w) write_url=true; shift 2 ;;' \
    '        -*) shift ;;' \
    '        *) url=$1; shift ;;' \
    '    esac' \
    'done' \
    'if [ "${write_url}" = true ]; then' \
    '    printf "%s\n" "https://github.com/pulkitxm/Gitbot/releases/tag/v0.3.0"' \
    '    exit 0' \
    'fi' \
    '[ -n "${destination}" ]' \
    'case "${url}" in' \
    '    */SHA256SUMS) cp "$GITBOT_TEST_FIXTURES/SHA256SUMS" "${destination}" ;;' \
    '    *) cp "$GITBOT_TEST_FIXTURES/$GITBOT_TEST_ASSET" "${destination}" ;;' \
    'esac' >"${FAKE_BIN}/curl"

chmod +x "${FAKE_BIN}/uname" "${FAKE_BIN}/curl"

run_installer() {
    test_home=$1
    test_os=$2
    test_arch=$3
    test_asset=$4
    shift 4
    mkdir -p "${test_home}"
    env \
        HOME="${test_home}" \
        PATH="${FAKE_BIN}:${ORIGINAL_PATH}" \
        GITBOT_TEST_OS="${test_os}" \
        GITBOT_TEST_ARCH="${test_arch}" \
        GITBOT_TEST_ASSET="${test_asset}" \
        GITBOT_TEST_FIXTURES="${FIXTURES}" \
        sh "${INSTALLER}" "$@"
}

printf 'test: installs a pinned Linux x86-64 release\n'
case_dir=${TEST_ROOT}/linux-x86
bin_dir=${case_dir}/bin
prepare_release gitbot-linux-x86_64 'linux x86 binary'
run_installer "${case_dir}/home" Linux x86_64 gitbot-linux-x86_64 \
    --version 0.1.0 --bin-dir "${bin_dir}" >"${case_dir}.out" 2>&1
assert_equals 'linux x86 binary' "$(cat "${bin_dir}/gitbot")"
[ -x "${bin_dir}/gitbot" ] || fail "installed binary is not executable"
assert_contains 'verified SHA-256 checksum' "${case_dir}.out"

printf 'test: installs the latest macOS Apple Silicon release\n'
case_dir=${TEST_ROOT}/macos-arm
bin_dir=${case_dir}/bin
prepare_release gitbot-macos-aarch64 'macOS ARM binary'
run_installer "${case_dir}/home" Darwin arm64 gitbot-macos-aarch64 \
    --bin-dir "${bin_dir}" >"${case_dir}.out" 2>&1
assert_equals 'macOS ARM binary' "$(cat "${bin_dir}/gitbot")"

printf 'test: rejects a checksum mismatch\n'
case_dir=${TEST_ROOT}/bad-checksum
bin_dir=${case_dir}/bin
printf 'tampered binary' >"${FIXTURES}/gitbot-linux-x86_64"
printf '%064d  gitbot-linux-x86_64\n' 0 >"${FIXTURES}/SHA256SUMS"
if run_installer "${case_dir}/home" Linux x86_64 gitbot-linux-x86_64 \
    --bin-dir "${bin_dir}" >"${case_dir}.out" 2>&1; then
    fail "checksum mismatch unexpectedly succeeded"
fi
assert_contains 'checksum verification failed' "${case_dir}.out"

printf 'test: rejects unsupported operating systems\n'
case_dir=${TEST_ROOT}/unsupported
if run_installer "${case_dir}/home" FreeBSD x86_64 gitbot-linux-x86_64 \
    --bin-dir "${case_dir}/bin" >"${case_dir}.out" 2>&1; then
    fail "unsupported operating system unexpectedly succeeded"
fi
assert_contains 'unsupported operating system: FreeBSD' "${case_dir}.out"
