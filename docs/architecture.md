# Architecture

> English original. German version: [architecture.de.md](architecture.de.md)

```
┌──────────────────────────── Linux desktop ─────────────────────────────┐
│  UI: Svelte 5 in the system WebView (Tauri 2), Fluent translations     │
│        │ Tauri commands                                                │
│  syno-mount-app   commands; also `run-mount <id>` and `agent`          │
│   ├─ syno-discovery   Bonjour + Synology Assistant search              │
│   ├─ syno-dsm         DSM Web API: sign-in, SFTP, user home, shares    │
│   ├─ syno-ssh         ed25519 keys, known_hosts, key install over SFTP │
│   └─ syno-mount       sshfs command, systemd units, status, self-test  │
│                                                                        │
│  systemd --user:  syno-mount-<id>.service → syno-mount run-mount <id>  │
│                   syno-mount-agent.service → syno-mount agent          │
└──────────────┬───────────────────────────────────┬─────────────────────┘
      HTTPS (5001), setup only            SFTP over SSH (22 or forwarded)
┌──────────────┴───────────────────────────────────┴─────────────────────┐
│  Synology NAS, DSM 7.2+                                                │
└────────────────────────────────────────────────────────────────────────┘
```

All crates except `syno-mount-app` are free of UI code, so Syno Companion can take them over. `syno-dsm` and `syno-discovery` are copies of Companion's `companion-dsm` and `companion-discovery`; `syno-dsm` adds the calls in `files.rs`.

## Setup

1. **DSM sign-in** (optional): `SYNO.API.Auth`, with two-step verification and certificate pinning as in Companion. It reads `SYNO.Core.FileServ.FTP.SFTP` and `SYNO.Core.User.Home` and switches them on after asking (only administrators may). The home service needs a volume (`location`), otherwise DSM answers 3103. Accounts DSM refuses (error 402) continue without DSM.
2. **Host key**: read with russh, shown as SHA-256 fingerprint, pinned under the alias `syno-mount.<host>_<port>` in `~/.local/share/syno-mount/known_hosts`. ssh uses it through `HostKeyAlias`, so the LAN and the internet address share one entry.
3. **Key**: ed25519, created from `/dev/urandom`, stored with mode 600; comment `syno-mount:<account>@<computer>`. It is appended to `authorized_keys` over SFTP with the password, then the app signs in with the key to prove that sshd accepts it.
4. **Folders**: DSM 7 puts every SFTP account in a chroot whose `/` holds the shared folders the account may open (administrators too), and the home folder is `/home`. The folders offered are what SFTP shows; DSM's list adds nothing SFTP cannot open.

The password lives only in memory while the assistant is open.

## Mounting

`run-mount <id>` loads the folder from `mounts.json`, tries the LAN address and then the internet address (TCP, 1.5 s each, for up to 30 s), notes the choice in `~/.local/state/syno-mount/<id>.json` and replaces itself with `sshfs -f`. Options:

- ssh: `IdentityFile`, `IdentitiesOnly`, `UserKnownHostsFile`, `HostKeyAlias`, `HostKeyAlgorithms`, `StrictHostKeyChecking=yes`, `BatchMode=yes`, `ServerAliveInterval=15`, `ServerAliveCountMax=3`, `Ciphers` (AES-GCM first on Intel/AMD, ChaCha20 first on ARM; commas escaped for FUSE), compression only over the internet.
- sshfs: `reconnect`, `max_conns` (sshfs 3.7+), `auto_unmount`, `fsname=syno-mount:<id>`, listing cache, and `idmap=file` with `uidfile`/`gidfile` that map the account's numbers on the NAS to the local user and group.

The service has `Restart=on-failure`, so systemd tries again every 10 seconds when the NAS is away.

An internet address can be **behind a reverse proxy** (`tunnel` in `mounts.json`). Then "reachable" means TLS to the proxy and the SSH banner of the NAS behind it, and sshfs gets `ProxyCommand='<program>' tunnel <id>`: `syno-mount tunnel` opens TLS with the ALPN protocol `syno-mount-ssh` (rustls, ring) and connects it to ssh's standard input and output. Everything else, the pinned host key included, stays as it is. See [https-tunnel.md](https-tunnel.md).

## Agent

Every 5 seconds the agent compares the time since boot (`/proc/uptime`, which counts suspend) with a monotonic clock that does not, and `/proc/net/route` with its last state. After a suspend or a route change, and every 30 seconds anyway, it checks every running folder: a mount that does not answer within 4 seconds is released (`fusermount3 -uz`) and its service restarted; a folder on the internet address moves back when the LAN answers again, and the other way round.

## Self-test

In a temporary folder inside the mount: write and read 1 MiB, `chmod 640`, owner, `mtime`, rename over a file, symlink and read back, hard link, `user.*` xattr, `flock`, and a sparse 5 GiB file. Locks always count as limited, because SFTP does not pass them on.

## Administrator area

A separate DSM sign-in that must be an administrator. It reads `SYNO.Core.User` (accounts), `SYNO.Core.Group.Member` (members of "administrators"), `SYNO.Core.AppPriv.Rule` for the application `SYNO.SFTP` (the rule for everyone and per-account rules), `SYNO.Core.Share` and, per shared folder, `SYNO.Core.Share.Permission`. Changes are collected in the interface and only sent after the user confirmed the list: an SFTP rule per account is set (`set`) or removed (`delete`, back to the rule for everyone), and the accounts' own permissions on a folder are written in one `set` call. "Custom" permissions from DSM's ACL editor are shown but not changed.

The sign-in uses the session name `FileStation`: DSM refuses accounts outside the administrators with error 402 for session names it does not know, while `FileStation` is open to every account and still allows the administrator APIs.

## Windows

Windows mounts with [rclone](https://rclone.org) through [WinFsp](https://winfsp.dev) instead of sshfs. SSHFS-Win was considered and left aside: its last release is from 2021. The rest is shared: assistant, key, pinned host key, administrator area, self-test.

- **Command:** `rclone mount :sftp,host=…,port=…,user=…,key_file=…,known_hosts_file=…,ciphers=…,shell_type=none,disable_hashcheck=true,set_modtime=true:/<share> Z: --network-mode --vfs-cache-mode writes …`. No rclone config file is written. rclone checks host keys by address, so the pinned key is also written under `host` and `[host]:port` for the LAN and the internet address.
- **Agent:** `syno-mount.exe agent` holds a lock file, so one runs per user. Every 2 seconds it starts an rclone for each folder that should be mounted (a `<id>.want` file written by the app, or "mount at sign-in"), restarts one that ended (after 5 seconds) or that hangs (checked every 30 seconds), moves back to the LAN when it answers again, and ends rclone for folders that were stopped or removed. rclone's messages go to `<id>.log`; the app shows the last lines when a folder fails.
- **Autostart:** while any folder should mount at sign-in, the value "Syno Mount" in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` starts the agent.
- **Self-test:** without permissions, owner, links, xattr and the 5 GiB file (rclone would upload all of it).
