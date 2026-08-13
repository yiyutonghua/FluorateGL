#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=trace-fixture-lib.sh
. "${script_dir}/trace-fixture-lib.sh"

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <trace-case> [fixture-dir]" >&2
  exit 2
fi

case_name="$1"
fixture_dir="${2:-tools/gl-conformance/trace_replay/fixtures}"
python_bin="${PYTHON:-python3}"
# Fixture mirrors (MobileGL's own mirror farm), tried in order before falling back to
# MobileGL's GitHub LFS. Override the whole list with FLUORATEGL_TRACE_FIXTURE_MIRROR_BASES
# (whitespace separated); FLUORATEGL_TRACE_FIXTURE_MIRROR_BASE still works and is tried
# first. The legacy MOBILEGL_TRACE_FIXTURE_MIRROR_* names are accepted as fallbacks so an
# existing MobileGL CI configuration carries over unchanged.
default_mirror_bases=(
  "https://git.hit.moe/swung0x48/MobileGL/media/branch/dev/tools/trace_replay/fixtures"
  "https://repo.miawa.cn/mgl/tools/trace_replay/fixtures"
)
if [ -n "${FLUORATEGL_TRACE_FIXTURE_MIRROR_BASES:-${MOBILEGL_TRACE_FIXTURE_MIRROR_BASES:-}}" ]; then
  read -r -a mirror_bases <<< "${FLUORATEGL_TRACE_FIXTURE_MIRROR_BASES:-${MOBILEGL_TRACE_FIXTURE_MIRROR_BASES}}"
else
  mirror_bases=("${default_mirror_bases[@]}")
fi
if [ -n "${FLUORATEGL_TRACE_FIXTURE_MIRROR_BASE:-${MOBILEGL_TRACE_FIXTURE_MIRROR_BASE:-}}" ]; then
  mirror_bases=("${FLUORATEGL_TRACE_FIXTURE_MIRROR_BASE:-${MOBILEGL_TRACE_FIXTURE_MIRROR_BASE}}" "${mirror_bases[@]}")
fi
# Optional bearer token for mirrors that require authentication (private Gitea).
mirror_token="${FLUORATEGL_TRACE_FIXTURE_MIRROR_TOKEN:-${MOBILEGL_TRACE_FIXTURE_MIRROR_TOKEN:-}}"
download_attempts="${FLUORATEGL_TRACE_FIXTURE_DOWNLOAD_ATTEMPTS:-${MOBILEGL_TRACE_FIXTURE_DOWNLOAD_ATTEMPTS:-5}}"
retry_delay="${FLUORATEGL_TRACE_FIXTURE_RETRY_DELAY:-${MOBILEGL_TRACE_FIXTURE_RETRY_DELAY:-2}}"

if ! command -v "${python_bin}" >/dev/null 2>&1 && command -v python >/dev/null 2>&1; then
  python_bin=python
fi

if ! [[ "${download_attempts}" =~ ^[1-9][0-9]*$ ]]; then
  echo "FLUORATEGL_TRACE_FIXTURE_DOWNLOAD_ATTEMPTS must be a positive integer: ${download_attempts}" >&2
  exit 2
fi
if ! [[ "${retry_delay}" =~ ^[0-9]+$ ]]; then
  echo "FLUORATEGL_TRACE_FIXTURE_RETRY_DELAY must be a non-negative integer: ${retry_delay}" >&2
  exit 2
fi

fixture_list="$("${python_bin}" tools/gl-conformance/trace_replay/trace_cases.py \
  --format fixture-files \
  --case "${case_name}" \
  --fixture-root "${fixture_dir}")"
# Strip CR so the script also works when python emits CRLF (Git Bash on Windows).
mapfile -t files < <(printf '%s\n' "${fixture_list}" | tr -d '\r')

include="$(IFS=,; echo "${files[*]}")"
if [ "${case_name}" = "OpenRA" ]; then
  # OpenRA fixtures are ordinary git blobs in the MobileGL repository (not LFS
  # pointers), so there is no oid metadata to verify against; they are fetched
  # directly from MobileGL's GitHub raw endpoint. The mirror media endpoints
  # only serve LFS objects, so they cannot be used here.
  mkdir -p "${fixture_dir}"
  for file in "${files[@]}"; do
    name="$(basename "${file}")"
    url="https://raw.githubusercontent.com/MobileGL-Dev/MobileGL/dev/tools/trace_replay/fixtures/${name}"
    echo "Fetching OpenRA fixture from GitHub raw: ${url}"
    if ! curl -L --fail --show-error --output "${file}.tmp" "${url}"; then
      echo "failed to download OpenRA fixture: ${url}" >&2
      rm -f "${file}.tmp"
      exit 1
    fi
    if [ ! -s "${file}.tmp" ] || head -n 1 "${file}.tmp" | grep -q "version https://git-lfs.github.com/spec/v1"; then
      echo "downloaded OpenRA fixture is empty or an LFS pointer: ${url}" >&2
      rm -f "${file}.tmp"
      exit 1
    fi
    mv "${file}.tmp" "${file}"
  done
  echo "Fetched OpenRA fixture files: ${include}"
  exit 0
fi

fetch_file_from_mirror() {
  local file="$1"
  local url="$2"
  local metadata
  local expected_oid
  local expected_size
  local tmp_file="${file}.tmp"
  local attempt
  local partial_size
  local curl_status
  local curl_auth

  metadata="$(get_lfs_metadata "${file}")" || return 1
  read -r expected_oid expected_size <<< "${metadata}"

  if [ -f "${tmp_file}" ]; then
    partial_size="$(wc -c < "${tmp_file}" | tr -d '[:space:]')"
    if [ "${partial_size}" -gt "${expected_size}" ]; then
      echo "Discarding oversized partial fixture ${tmp_file}: ${partial_size} > ${expected_size}" >&2
      rm -f "${tmp_file}"
    elif [ "${partial_size}" = "${expected_size}" ]; then
      if verify_fixture_file "${tmp_file}" "${file}" "${expected_oid}" "${expected_size}"; then
        mv "${tmp_file}" "${file}"
        return 0
      fi
      rm -f "${tmp_file}"
    fi
  fi

  for ((attempt = 1; attempt <= download_attempts; attempt++)); do
    partial_size=0
    if [ -f "${tmp_file}" ]; then
      partial_size="$(wc -c < "${tmp_file}" | tr -d '[:space:]')"
    fi

    if [ "${partial_size}" -gt 0 ]; then
      echo "Resuming mirror download for ${file} at byte ${partial_size} (attempt ${attempt}/${download_attempts})"
    else
      echo "Starting mirror download for ${file} (attempt ${attempt}/${download_attempts})"
    fi

    curl_auth=()
    if [ -n "${mirror_token}" ]; then
      curl_auth=(--header "Authorization: token ${mirror_token}")
    fi
    if curl -L --fail --show-error --continue-at - "${curl_auth[@]}" --output "${tmp_file}" "${url}"; then
      if verify_fixture_file "${tmp_file}" "${file}" "${expected_oid}" "${expected_size}"; then
        mv "${tmp_file}" "${file}"
        return 0
      fi
      echo "Mirror download failed integrity verification; retrying from the beginning: ${file}" >&2
      rm -f "${tmp_file}"
    else
      curl_status=$?
      partial_size=0
      if [ -f "${tmp_file}" ]; then
        partial_size="$(wc -c < "${tmp_file}" | tr -d '[:space:]')"
      fi

      if [ "${partial_size}" = "${expected_size}" ]; then
        if verify_fixture_file "${tmp_file}" "${file}" "${expected_oid}" "${expected_size}"; then
          mv "${tmp_file}" "${file}"
          return 0
        fi
        rm -f "${tmp_file}"
        partial_size=0
      elif [ "${partial_size}" -gt "${expected_size}" ]; then
        echo "Discarding oversized partial fixture ${tmp_file}: ${partial_size} > ${expected_size}" >&2
        rm -f "${tmp_file}"
        partial_size=0
      elif [ "${curl_status}" -eq 33 ]; then
        echo "Mirror refused the resume request; retrying from the beginning: ${file}" >&2
        rm -f "${tmp_file}"
        partial_size=0
      fi

      echo "Mirror download attempt ${attempt}/${download_attempts} failed with curl exit ${curl_status}; retained ${partial_size} bytes for resume: ${file}" >&2
    fi

    if [ "${attempt}" -lt "${download_attempts}" ]; then
      sleep "${retry_delay}"
    fi
  done

  rm -f "${tmp_file}"
  return 1
}

# Files no mirror could serve, even after retrying every mirror. Only these fall
# back to MobileGL's GitHub LFS, so a mirror that served the rest of the case
# still spares GitHub the bandwidth for those files.
mirror_failures=()

fetch_from_mirror() {
  mkdir -p "${fixture_dir}"
  for file in "${files[@]}"; do
    local name
    local url
    local base
    local fetched=0
    name="$(basename "${file}")"
    for base in "${mirror_bases[@]}"; do
      url="${base%/}/${name}"
      echo "Fetching trace fixture from mirror: ${url}"
      if fetch_file_from_mirror "${file}" "${url}"; then
        fetched=1
        break
      fi
      echo "Mirror did not serve ${name}; trying the next mirror" >&2
    done
    if [ "${fetched}" -ne 1 ]; then
      mirror_failures+=("${file}")
    fi
  done
  [ "${#mirror_failures[@]}" -eq 0 ]
}

# Falls back to MobileGL's GitHub LFS media endpoint (public repository, no
# authentication), then verifies the download against the pointer metadata the
# same way the mirror path does.
fetch_from_github_lfs() {
  local file
  local name
  local metadata
  local expected_oid
  local expected_size
  local url
  local remaining=("${mirror_failures[@]}")
  mirror_failures=()
  for file in "${remaining[@]}"; do
    name="$(basename "${file}")"
    metadata="$(get_lfs_metadata "${file}")"
    read -r expected_oid expected_size <<< "${metadata}"
    url="https://media.githubusercontent.com/media/MobileGL-Dev/MobileGL/dev/tools/trace_replay/fixtures/${name}"
    echo "Fetching trace fixture from MobileGL GitHub LFS: ${url}"
    if curl -L --fail --show-error --output "${file}.tmp" "${url}" \
      && verify_fixture_file "${file}.tmp" "${file}" "${expected_oid}" "${expected_size}"; then
      mv "${file}.tmp" "${file}"
      echo "Fetched ${name} from MobileGL GitHub LFS"
    else
      rm -f "${file}.tmp"
      mirror_failures+=("${file}")
      echo "MobileGL GitHub LFS did not serve ${name}" >&2
    fi
  done
  [ "${#mirror_failures[@]}" -eq 0 ]
}

if fetch_from_mirror; then
  echo "Fetched trace fixture files for ${case_name} from mirror: ${include}"
elif fetch_from_github_lfs; then
  echo "Fetched trace fixture files for ${case_name} from MobileGL GitHub LFS: ${include}"
else
  fallback_include="$(IFS=,; echo "${mirror_failures[*]}")"
  echo "Mirror and GitHub LFS both failed for ${#mirror_failures[@]} of ${#files[@]} file(s) of ${case_name}: ${fallback_include}" >&2
  exit 1
fi

for file in "${files[@]}"; do
  metadata="$(get_lfs_metadata "${file}")"
  read -r expected_oid expected_size <<< "${metadata}"
  verify_fixture_file "${file}" "${file}" "${expected_oid}" "${expected_size}"
done
