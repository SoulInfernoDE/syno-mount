# Releasing

> English original. German version: [releasing.de.md](releasing.de.md)

Releases are built by GitHub Actions ([release.yml](../.github/workflows/release.yml)) when a `v*` tag is pushed.

| Platform     | Runner             | Packages                    |
| ------------ | ------------------ | --------------------------- |
| Linux x86_64 | Ubuntu 22.04       | `.deb`, `.rpm`, `.AppImage` |
| Linux arm64  | Ubuntu 22.04 (arm) | `.deb`, `.rpm`, `.AppImage` |

Every release also carries `SHA256SUMS.txt`. Versions before 1.0 and tags with `alpha`, `beta` or `rc` are marked as pre-releases.

## Steps

1. Set the new version in `Cargo.toml` (`[workspace.package]`) and `ui/package.json`, then run `cargo check` so `Cargo.lock` follows. The release job refuses a tag that does not match both.
2. Write the notes in `docs/release-notes/v<version>.md`: English first, then a `## Deutsch` section. Without this file a generic text in both languages is used.
3. Commit, tag and push:

   ```bash
   git tag v0.5.0
   git push origin main v0.5.0
   ```

4. Check the release on GitHub when the workflow is done.

A manual run of the workflow (_Actions › Release › Run workflow_) builds all packages and keeps them as workflow artifacts without publishing anything; use it to test packaging changes.

## Not yet

- **Signing:** packages are not signed yet.
- **Flatpak:** not planned; FUSE mounts from a sandbox need workarounds. AUR is planned.
