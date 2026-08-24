# Self-update

`pukbot update` checks the latest stable GitHub Release and replaces the
running executable only when a newer version exists.

```bash
pukbot update
pukbot update --check
pukbot update --check --json
```

The update sequence is fixed:

1. Fetch the latest release metadata from the Pukbot GitHub repository.
2. Parse the stable semantic version tag.
3. Select the release asset for the current operating system and architecture.
4. Download the release checksum document and binary with size limits and a
   30-second network timeout.
5. Require exactly one valid SHA-256 entry for the selected asset.
6. Verify the binary before touching the installed executable.
7. Preserve executable permissions and replace the resolved executable.

Supported targets are Linux x86-64 and AArch64, macOS x86-64 and Apple Silicon,
and Windows x86-64. Windows ARM64 uses the published x86-64 executable.

`--check` performs metadata lookup without downloading or replacing a binary.
JSON output reports `up_to_date`, `available`, or `updated`, the current and
latest versions, and the selected asset when an update is available.

The updater is intended for release binaries and installer copies. If a
package manager owns the executable, use that package manager to preserve its
installation records.
