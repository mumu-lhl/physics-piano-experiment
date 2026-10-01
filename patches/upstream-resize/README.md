# Upstream resize patches

These patches isolate the local editor-resizing changes from the vendored source copies.
`tools/sync_upstream_vendor.sh` downloads the pinned upstream sources, verifies the
`nice-plug` crate checksum, applies these patches, then replaces the managed directories
under `vendor/`.

## Upstream revisions

- `vizia-plug.patch` targets `vizia-plug` commit `34812c0ba14c5df39621bab956c74e660220636b` from `https://github.com/vizia/vizia-plug.git`.
- `nice-plug.patch` targets the `nice-plug` 0.1.10 crate from crates.io. Its registry checksum is `a5158967c27b26738f339bfd912cf386a3d88241a4111e8a79a8e42aa055c8b5`.

## Syncing and applying

Run the sync script from any directory in the repository:

```sh
./tools/sync_upstream_vendor.sh
```

To update upstream, change the pinned revision/version and checksum in the script and here,
then review whether each patch still applies. The script fails before replacing `vendor/`
if fetching, checksum verification, or patch application fails. Do not edit the generated
`vendor/` copies directly; put local changes in these patch files.
