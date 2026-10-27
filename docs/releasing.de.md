# Releases veröffentlichen

> Deutsche Fassung. Englisches Original: [releasing.md](releasing.md)

Releases baut GitHub Actions ([release.yml](../.github/workflows/release.yml)), sobald ein `v*`-Tag gepusht wird.

| Plattform    | Runner             | Pakete                      |
| ------------ | ------------------ | --------------------------- |
| Linux x86_64 | Ubuntu 22.04       | `.deb`, `.rpm`, `.AppImage` |
| Linux arm64  | Ubuntu 22.04 (arm) | `.deb`, `.rpm`, `.AppImage` |

Jedes Release enthält außerdem `SHA256SUMS.txt`. Versionen vor 1.0 und Tags mit `alpha`, `beta` oder `rc` werden als Vorabversion markiert.

## Schritte

1. Die neue Version in `Cargo.toml` (`[workspace.package]`) und `ui/package.json` eintragen, dann `cargo check` ausführen, damit `Cargo.lock` nachzieht. Der Release-Job lehnt einen Tag ab, der nicht zu beiden passt.
2. Die Notizen in `docs/release-notes/v<version>.md` schreiben: zuerst Englisch, dann ein Abschnitt `## Deutsch`. Ohne diese Datei wird ein allgemeiner Text in beiden Sprachen verwendet.
3. Committen, taggen und pushen:

   ```bash
   git tag v0.5.0
   git push origin main v0.5.0
   ```

4. Wenn der Workflow fertig ist, das Release auf GitHub prüfen.

Ein manueller Lauf des Workflows (_Actions › Release › Run workflow_) baut alle Pakete und behält sie als Workflow-Artefakte, ohne etwas zu veröffentlichen; gedacht zum Testen von Änderungen an der Paketierung.

## Noch nicht

- **Signieren:** Die Pakete sind noch nicht signiert.
- **Flatpak:** nicht geplant; FUSE-Mounts aus einer Sandbox brauchen Umwege. AUR ist geplant.
