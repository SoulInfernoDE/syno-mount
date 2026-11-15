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
3. **Key**: ed25519, created from `/dev/urandom`, stored with mode 600; comment `syno-mount:<account>@<computer>`. It is appended to `authorized_keys` over SFTP with the password, then the app signs in with the key to prove that sshd accepts it. When sshd refuses it, the still open password session reads the modes of the home, `.ssh` and `authorized_keys`: if group or others may write one of them (Windows ACLs show as `777`), OpenSSH's `StrictModes` is the cause and the error `key-refused-acl-home` sends the user to an administrator; otherwise it is `key-refused`. Either way the password worked, so the message no longer blames it.
4. **Folders**: DSM 7 puts every SFTP account in a chroot whose `/` holds the shared folders the account may open (administrators too), and the home folder is `/home`. The folders offered are what SFTP shows; DSM's list adds nothing SFTP cannot open.

The password lives only in memory while the assistant is open.

## Mounting

An address counts only when SSH answers there: the first bytes must be the SSH banner, so another device with the same private address in a foreign network does not count (LAN 1.5 s, internet 5 s per step; behind a proxy, TLS and the banner behind it). Every address of a name is tried, IPv6 and IPv4. When none answers, the journal says why for each one, for example `LAN 192.168.1.20:22: timed out; internet nas.example.org:2222: connection refused`, or that no internet address is set.

`run-mount <id>` loads the folder from `mounts.json`, tries the LAN address and then the internet address for up to 30 s, notes the choice in `~/.local/state/syno-mount/<id>.json` and replaces itself with `sshfs -f`. Options:

- ssh: `IdentityFile`, `IdentitiesOnly`, `UserKnownHostsFile`, `HostKeyAlias`, `HostKeyAlgorithms`, `StrictHostKeyChecking=yes`, `BatchMode=yes`, `ServerAliveInterval=15`, `ServerAliveCountMax=3`, `Ciphers` (AES-GCM first on Intel/AMD, ChaCha20 first on ARM; commas escaped for FUSE), compression only over the internet.
- sshfs: `reconnect`, `max_conns` (sshfs 3.7+), `auto_unmount`, `fsname=syno-mount:<id>`, listing cache, and `idmap=file` with `uidfile`/`gidfile` that map the account's numbers on the NAS to the local user and group.

The service has `Restart=on-failure`, so systemd tries again every 10 seconds when the NAS is away.

An internet address can be **behind a reverse proxy** (`tunnel` in `mounts.json`). Then "reachable" means TLS to the proxy and the SSH banner of the NAS behind it, and sshfs gets `ProxyCommand='<program>' tunnel <id>`: `syno-mount tunnel` opens TLS with the ALPN protocol `syno-mount-ssh` (rustls, ring) and connects it to ssh's standard input and output. Everything else, the pinned host key included, stays as it is. See [https-tunnel.md](https-tunnel.md).

## Agent

Every 5 seconds the agent compares the time since boot (`/proc/uptime`, which counts suspend) with a monotonic clock that does not, and `/proc/net/route` with its last state. After a suspend or a route change, and every 30 seconds anyway, it checks every running folder: a mount that does not answer within 4 seconds is released (`fusermount3 -uz`) and its service restarted; a folder on the internet address moves back when the LAN answers again, and the other way round.

## Self-test

In a temporary folder inside the mount: write and read 1 MiB, `chmod 640`, owner, `mtime`, rename over a file, symlink and read back, hard link, `user.*` xattr, `flock`, and a sparse 5 GiB file. Locks always count as limited, because SFTP does not pass them on.

## Administrator area

A separate DSM sign-in that must be an administrator. It reads `SYNO.Core.User` (accounts), `SYNO.Core.Group.Member` (members of "administrators"), `SYNO.Core.AppPriv.Rule` for the application `SYNO.SFTP` (the rule for everyone and per-account rules), `SYNO.Core.Share` and, per shared folder, `SYNO.Core.Share.Permission`. The view by account reads `SYNO.Core.Share.Permission` `list_by_user`: per shared folder the account's own permission and, in `inherit`, what its groups and the folder's ACL give it (`rw`, `ro`, `na` for no access, `-` for nothing). The preview applies DSM's rule (no access wins, otherwise the wider permission) to the edits not sent yet and adds `/home` when the home service is on; with SFTP denied it shows nothing. Changes are collected in the interface and only sent after the user confirmed the list: an SFTP rule per account is set (`set`) or removed (`delete`, back to the rule for everyone), and the accounts' own permissions on a folder are written in one `set` call. "Custom" permissions from DSM's ACL editor are shown but not changed.

"Prepare key login" runs a short script as root, since only root can remove the ACLs of a home folder. The Web API reaches root only through the Task Scheduler, in the calls DSM's own Control Panel makes (DSM 7.4.1): `SYNO.Core.User.PasswordConfirm` `auth` with the administrator's password gives a short-lived `SynoConfirmPWToken`; `SYNO.Core.TaskScheduler.Root` `create` (version 4) makes a disabled user-defined script task owned by root with that token (DSM's task service aborts when a schedule field is missing, so all are sent); `SYNO.Core.TaskScheduler` `run` starts it; `get_history_status_list` gives its exit code once `exit_type` is no longer `running`; `delete` removes the task, also when running it failed. The script's output is not read, since DSM keeps it only with a global setting Syno Mount does not change; each step has its own exit code instead, and the last one checks the modes. The script is shown to the administrator before it runs.

The account owns its home folder and could put links there that root would follow (`.ssh -> /etc`, `authorized_keys -> /etc/shadow`). So the script first takes the home folder (owner root, 755, no ACL) and then `.ssh` (owner root, 700); from then on the account cannot add or swap anything in them. It refuses `.ssh` or `authorized_keys` when they are links, not a plain folder or file, or a file with more than one hard link (exit 15), uses `chown -h`, and gives both folders back to the account at the end, also when a step fails (a `trap`). The account name goes into the script once, quoted, as a variable. If the task cannot be deleted, the error says so and names it, before any other reason. Tested on Virtual DSM with `.ssh` as a link to a root folder, `authorized_keys` as a link to a root file and as a hard link to another account's key file: each refused, the targets unchanged.

The sign-in uses the session name `FileStation`: DSM refuses accounts outside the administrators with error 402 for session names it does not know, while `FileStation` is open to every account and still allows the administrator APIs.

## Encrypted DNS

The status of a mounted folder says how the name of the address in use is looked up (`dns.rs`, cached for a minute): no query for an IP address or a name in the hosts file; multicast DNS for `.local`; on Linux with systemd-resolved, the DNS over TLS mode of the link that answered (`resolvectl query`, `resolvectl dnsovertls`: `yes` encrypted, `opportunistic` maybe not); another resolver on `127.0.0.1` cannot be judged; anything else is plain DNS. On Windows, PowerShell reads the DNS servers of the interface with the default route and whether DNS over HTTPS is set up for each (`Get-DnsClientDohServerAddress`, `DohFlags` per interface, `EnableAutoDoh`). Only the way to the resolver is judged; the resolver itself still sees the name.

## Windows

Windows mounts with [rclone](https://rclone.org) through [WinFsp](https://winfsp.dev) instead of sshfs. SSHFS-Win was considered and left aside: its last release is from 2021. The rest is shared: assistant, key, pinned host key, administrator area, self-test.

- **Command:** `rclone mount :sftp,host=…,port=…,user=…,key_file=…,known_hosts_file=…,ciphers=…,shell_type=none,disable_hashcheck=true,set_modtime=true:/<share> Z: --network-mode --vfs-cache-mode writes …`. No rclone config file is written. rclone checks host keys by address, so the pinned key is also written under `host` and `[host]:port` for the LAN and the internet address.
- **Agent:** `syno-mount.exe agent` holds a lock file, so one runs per user. Every 2 seconds it starts an rclone for each folder that should be mounted (a `<id>.want` file written by the app, or "mount at sign-in"), restarts one that ended (after 5 seconds) or that hangs (checked every 30 seconds), moves back to the LAN when it answers again, and ends rclone for folders that were stopped or removed. rclone's messages go to `<id>.log`; the app shows the last lines when a folder fails.
- **Autostart:** while any folder should mount at sign-in, the value "Syno Mount" in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` starts the agent.
- **Self-test:** without permissions, owner, links, xattr and the 5 GiB file (rclone would upload all of it).
