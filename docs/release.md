# Cutting a release

## Steps

1. Make sure `main` is green in CI.
2. Tag it `vX.Y.Z` (three plain numbers) and push the tag:

   ```sh
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

3. `.github/workflows/release.yml` builds Windows x64, a universal macOS binary and Linux x64, packages them and creates
   the GitHub release with generated notes. Check the release page when it finishes.

A tag with a suffix (`v1.2.3-rc1`) becomes a pre-release: the "latest" link and the update check ignore it.

## What gets published

| File | Made by |
|---|---|
| `tidewater-X.Y.Z-windows-x64-setup.exe` | Inno Setup, from `packaging/windows/tidewater.iss`. Per-user install, Start menu entry, uninstaller. |
| `tidewater-X.Y.Z-macos.dmg` | `packaging/macos/build-dmg.sh` (`hdiutil`): `Tidewater.app` plus an Applications link. Unsigned and not notarized, so first launch is right-click, Open. The binary is the arm64 and x86_64 builds joined with `lipo`. |
| `tidewater-X.Y.Z-linux-x64.tar.gz` | The binary, `LICENSE` and `THIRD_PARTY_NOTICES.md`. No AppImage yet. |

The asset names are what the update check looks for (`update.rs`), so keep them. Only the program is packaged, never
game data.

The tag's number is stamped into the binary through the `TIDEWATER_VERSION` environment variable at build time
(`option_env!`). A build without it reports "dev" and never checks for updates. The version in `Cargo.toml` is not used
for this.

To try the macOS packaging on your own machine: build, then `packaging/macos/build-dmg.sh 0.0.0-test target/release/tidewater out.dmg`.

## The update check needs a public repository

The check calls GitHub's `releases/latest` API for the repository the release was built from (`TIDEWATER_REPO`,
set by the workflow), without signing in. While the
repository is private that answers 404, so no update is ever offered (the failure is silent; set `TIDEWATER_DEBUG=1`
to see it). Once the repository is public it starts working with no change. Release downloads need the repository to be
public for the same reason.

There is no landing page: the README and the GitHub releases page are where players start.
