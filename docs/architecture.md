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

## Agent

Every 5 seconds the agent compares the time since boot (`/proc/uptime`, which counts suspend) with a monotonic clock that does not, and `/proc/net/route` with its last state. After a suspend or a route change, and every 30 seconds anyway, it checks every running folder: a mount that does not answer within 4 seconds is released (`fusermount3 -uz`) and its service restarted; a folder on the internet address moves back when the LAN answers again, and the other way round.

## Self-test

In a temporary folder inside the mount: write and read 1 MiB, `chmod 640`, owner, `mtime`, rename over a file, symlink and read back, hard link, `user.*` xattr, `flock`, and a sparse 5 GiB file. Locks always count as limited, because SFTP does not pass them on.
