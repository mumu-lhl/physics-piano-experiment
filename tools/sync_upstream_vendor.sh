#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$script_dir/.." && pwd)
vendor_root="$repo_root/vendor"

# Keep these pins in sync with patches/upstream-resize/README.md.
vizia_plug_revision=34812c0ba14c5df39621bab956c74e660220636b
nice_plug_version=0.1.10
nice_plug_checksum=a5158967c27b26738f339bfd912cf386a3d88241a4111e8a79a8e42aa055c8b5

for required_command in git curl tar sha256sum patch; do
    if ! command -v "$required_command" >/dev/null 2>&1; then
        printf 'Missing required command: %s\n' "$required_command" >&2
        exit 1
    fi
done

mkdir -p "$vendor_root"
sync_stage=$(mktemp -d "$vendor_root/.upstream-sync.XXXXXX")
sync_tmp=$(mktemp -d "${TMPDIR:-/tmp}/physics-piano-upstream-sync.XXXXXX")
backup_dir="$sync_stage/backup"
new_vendor_dir="$sync_stage/new"
mkdir -p "$backup_dir" "$new_vendor_dir/nice-plug" "$new_vendor_dir/vizia-plug"

nice_plug_backed_up=0
vizia_plug_backed_up=0
nice_plug_installed=0
vizia_plug_installed=0
sync_completed=0

cleanup() {
    local exit_status=$?
    trap - EXIT

    if (( sync_completed == 0 )); then
        if (( nice_plug_installed == 1 )); then
            rm -rf -- "$vendor_root/nice-plug"
        fi
        if (( vizia_plug_installed == 1 )); then
            rm -rf -- "$vendor_root/vizia-plug"
        fi
        if (( nice_plug_backed_up == 1 )); then
            mv -- "$backup_dir/nice-plug" "$vendor_root/nice-plug"
        fi
        if (( vizia_plug_backed_up == 1 )); then
            mv -- "$backup_dir/vizia-plug" "$vendor_root/vizia-plug"
        fi
    fi

    rm -rf -- "$sync_stage" "$sync_tmp"
    exit "$exit_status"
}
trap cleanup EXIT

printf 'Fetching vizia-plug %s...\n' "$vizia_plug_revision"
git clone --quiet --no-checkout https://github.com/vizia/vizia-plug.git "$sync_tmp/vizia-plug"
git -C "$sync_tmp/vizia-plug" checkout --quiet --detach "$vizia_plug_revision"
git -C "$sync_tmp/vizia-plug" archive --format=tar "$vizia_plug_revision" \
    | tar -xf - -C "$new_vendor_dir/vizia-plug"

printf 'Fetching nice-plug %s...\n' "$nice_plug_version"
nice_plug_archive="$sync_tmp/nice-plug-$nice_plug_version.crate"
curl --fail --location --silent --show-error \
    "https://crates.io/api/v1/crates/nice-plug/$nice_plug_version/download" \
    --output "$nice_plug_archive"
if ! printf '%s  %s\n' "$nice_plug_checksum" "$nice_plug_archive" \
    | sha256sum --check --status; then
    printf 'Checksum verification failed for nice-plug %s.\n' "$nice_plug_version" >&2
    exit 1
fi
tar -xzf "$nice_plug_archive" --strip-components=1 -C "$new_vendor_dir/nice-plug"

printf 'Applying local resize patches...\n'
patch --batch --forward --fuzz=0 -p1 -d "$new_vendor_dir/vizia-plug" \
    < "$repo_root/patches/upstream-resize/vizia-plug.patch"
patch --batch --forward --fuzz=0 -p1 -d "$new_vendor_dir/nice-plug" \
    < "$repo_root/patches/upstream-resize/nice-plug.patch"

# Stage both patched sources first, then replace only the two managed vendor directories.
if [[ -e "$vendor_root/nice-plug" ]]; then
    mv -- "$vendor_root/nice-plug" "$backup_dir/nice-plug"
    nice_plug_backed_up=1
fi
if [[ -e "$vendor_root/vizia-plug" ]]; then
    mv -- "$vendor_root/vizia-plug" "$backup_dir/vizia-plug"
    vizia_plug_backed_up=1
fi

mv -- "$new_vendor_dir/nice-plug" "$vendor_root/nice-plug"
nice_plug_installed=1
mv -- "$new_vendor_dir/vizia-plug" "$vendor_root/vizia-plug"
vizia_plug_installed=1
sync_completed=1

printf 'Updated vendor/nice-plug and vendor/vizia-plug with the pinned upstream sources and local patches.\n'
