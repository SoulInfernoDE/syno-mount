# Roadmap

> English original. German version: [ROADMAP.de.md](ROADMAP.de.md)

**Syno Mount** (working title: Synology SSHFS Assistant) is a guided assistant that mounts Synology shared folders on Linux over **SSHFS**: set up once, mounted at every login, reconnected after suspend or a network change, over the LAN and over the internet.

**Why SSHFS:** Unix permissions and timestamps survive, unlike WebDAV, which proved unstable and limited. One encrypted port works over the internet without a VPN. NFSv4 and iSCSI follow later as further backends (see [Later](#later)).

**Relation to Syno Companion:** the same mount logic is needed later in [Syno Companion](https://github.com/SoulInfernoDE/synology-companion-app). This app therefore uses the same stack and reuses Companion's crates, so its crates and pages can move into Companion without a rewrite.

## Decisions

| Topic         | Decision                                                                                                      | Reason                                                                                                                                                                                  |
| ------------- | ------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Stack         | Tauri 2, Svelte 5, Rust (edition 2024), Fluent                                                                | Same as Companion: UI pages and crates can be moved over                                                                                                                                |
| Platform      | Linux only (x86_64, arm64)                                                                                    | SSHFS is reliable only with Linux FUSE; macFUSE and SSHFS-Win are not                                                                                                                   |
| Reused crates | `companion-discovery` and `companion-dsm`, copied as `syno-discovery` and `syno-dsm`                          | Finding NAS, signing in with 2FA and certificate pinning already work and are tested; `syno-dsm` needed calls that Companion does not expose yet, so a copy instead of a git dependency |
| New crates    | `syno-ssh` (keys, host keys, key install), `syno-mount` (backend trait, SSHFS backend, systemd units, status) | Planned in Companion as `companion-ssh` and the mount part of v0.2                                                                                                                      |
| Mount tool    | System `sshfs` (3.x) and `fusermount3`, not bundled                                                           | Distribution packages get security updates; the deb/rpm depends on `sshfs` and `fuse3`                                                                                                  |
| SSH settings  | Passed as `-o` options per mount; the app keeps its own `known_hosts` and keys                                | The user's `~/.ssh/config` stays untouched, removal is clean                                                                                                                            |
| Persistence   | One systemd **user** service per mount                                                                        | No root needed; root only for the optional "mount on access" mode (fstab, later)                                                                                                        |
| Packaging     | deb, rpm, AppImage; no Flatpak                                                                                | FUSE mounts from a sandbox need workarounds                                                                                                                                             |
| Licence       | GPL-3.0-or-later                                                                                              | Same as Companion                                                                                                                                                                       |
| DSM           | 7.2 and later                                                                                                 | The test VM runs DSM 7.2.2-72806; Companion targets 7.3+, so 7.2 support needs checking where APIs differ                                                                               |

## Findings from the test VM (DSM 7.2.2)

- SSH and SFTP are **separate switches**: `SYNO.Core.Terminal` (SSH) and `SYNO.Core.FileServ.FTP.SFTP` (`enable`, `portnum`). Both are off on a fresh DSM.
- The **user home service** (`SYNO.Core.User.Home`) is off by default; `/var/services/homes` then points to `@fake_home_link` and there is no place for `~/.ssh/authorized_keys`. The assistant has to switch it on (after asking).
- sshd is OpenSSH 8.2p1 with `Subsystem sftp internal-sftp -u 000`: the umask is 000, so the modes the client asks for are kept as sent. `PasswordAuthentication yes` is the default.
- The VM has an x86_64 CPU with AES-NI, so `aes128-gcm@openssh.com` is the fast cipher there.

Tested since (details in [test environment](docs/test-environment.md)): DSM 7 puts **every** SFTP account in a chroot, administrators too, so shares are always `/<share>` and the home is `/home`. `chmod` is ignored in shares with Windows ACLs (DSM's default) and works in folders without ACL. Symbolic links are created but not read back by DSM's SFTP server. The home service needs a volume when switched on (otherwise error 3103). Accounts without DSM access (error 402) still work over SFTP.

## v0.1 – Foundation

- [x] Repository: README (English, German), LICENSE, this roadmap, `docs/architecture.md`, `docs/test-environment.md`, each with a German version
- [x] Cargo workspace and Tauri 2 app shell, Svelte 5 UI, Fluent catalogs (`en` base and fallback, `de`)
- [x] Skins and light/dark mode taken from Companion
- [x] Find NAS and sign in through `companion-discovery` and `companion-dsm` (2FA, certificate pinning, keychain)
- [x] Check the system: `sshfs` and `fusermount3` present, version ≥ 3.7, user may use FUSE; tell the user how to install what is missing
- [x] CI: build, `clippy`, tests, licence check (`cargo deny`)

## v0.2 – Set up

- [x] Check through the DSM Web API whether SSH, SFTP and the user home service are on; switch them on after asking, showing what changes
- [x] SSH port: read it from DSM, warn about port 22 when the NAS is reachable from the internet
- [x] Key: create one ed25519 key per NAS and account (no passphrase, file mode 600, in the app's data folder) or use an existing key
- [x] Install the key: append one line to `~/.ssh/authorized_keys` on the NAS, marked with a comment so it can be removed later; set the permissions DSM expects. Works also for users without shell access, over SFTP
- [x] Host key: fetch it, show the SHA-256 fingerprint, pin it in the app's own `known_hosts` once the user trusts it; a changed key stops every mount with a warning
- [x] Shares: list them through `SYNO.FileStation.List list_share`; choose shares and mount folders (default `~/NAS/<NAS>/<share>`)
- [x] Paths: detect whether the account sees `/volume1/<share>` or `/<share>`
- [x] Mount and unmount by hand from the app
- [x] **Self-test** after mounting: create, `chmod`, rename over an existing file, symlink, hardlink, large file, xattr and lock checks. The result is a table of what works and what does not (xattrs and locks do not work over SFTP), so the user knows in advance which programs will have trouble
- [x] Undo per mount and for everything: stop the mount, remove units and bookmarks, remove the key line from the NAS

## v0.3 – Stay mounted

- [x] One systemd user service per mount (`sshfs -f`, `Restart=on-failure`, `fusermount3 -u` on stop), enabled at login
- [x] Default options: `reconnect`, `ServerAliveInterval=15`, `ServerAliveCountMax=3`, `idmap=user`, `IdentitiesOnly=yes`, `StrictHostKeyChecking=yes`
- [x] Small background agent (user service): listens on D-Bus to logind (`PrepareForSleep`) and NetworkManager; after resume or a network change it releases hanging mounts (`fusermount3 -uz`) and mounts again
- [x] Two addresses per NAS: LAN and internet (DDNS name or public address); the agent picks the one that answers, LAN first. QuickConnect is not possible, because it does not relay SSH
- [x] Bookmarks in the file manager (`~/.config/gtk-3.0/bookmarks`, used by Nemo and Nautilus)
- [x] Status page: per mount connected or not, address in use, latency; buttons to reconnect, unmount, open in the file manager
- [ ] Tray icon with status and notifications (optional)

## v0.4 – Fast

- [x] Choose the cipher from the NAS CPU (`companion-dsm` CPU info): `aes128-gcm@openssh.com` with AES-NI, `chacha20-poly1305@openssh.com` on ARM
- [x] `max_conns` for parallel transfers (sshfs ≥ 3.7); compression off on the LAN, optional over the internet
- [x] Cache settings for directory listings, with a short explanation of the trade-off when other computers change the same files
- [x] Speed test in the app: throughput (large file) and many small files, LAN and internet, to compare settings

## v0.5 – Release

- [x] Release builds for Linux x86_64 and arm64 (deb, rpm, AppImage); the AppImage checks for `sshfs` on start
- [x] Screenshots and README in English and German
- [ ] Full run on a real computer with systemd: set up → mount at login → suspend/resume → network change → undo everything (the chain without systemd is tested against the VM; see [test environment](docs/test-environment.md))
- [x] Release notes in English with a German section

## Later

- [ ] **NFSv4.1** backend: real UID/GID and locking on the LAN; over the internet only through a VPN (for example Tailscale from Package Center)
- [ ] **iSCSI** backend: a LUN as a local disk with any file system (ext4, Btrfs, NTFS) for games, Steam libraries and Wine prefixes; one computer at a time, the app prevents double use
- [ ] "Mount on access" through fstab with `x-systemd.automount` (needs root, through `pkexec`, with a preview of the change)
- [ ] Limit the installed key to SFTP (research: options in `authorized_keys` that DSM's sshd honours)
- [ ] Move `syno-ssh` and `syno-mount` into Syno Companion and use them there (Companion roadmap v0.2 mounts and v0.3 SSH)

## Open questions

- Tray icon: worth it, now that the agent keeps folders mounted without the window?
- Limit the installed key to SFTP, see [Later](#later).
