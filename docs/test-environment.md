# Test environment

> English original. German version: [test-environment.de.md](test-environment.de.md)

Syno Mount is tested **without a production NAS**, on three levels.

1. **Unit tests** (`cargo test`): command line, units, mountinfo parsing, owner maps, key files, `authorized_keys` editing, bookmarks, and the self-test against a local folder.
2. **Browser preview** (`npm run dev`, then `http://localhost:5175/?mock`): the whole interface with sample data, including the assistant (password `demo`). `?mock=empty` starts without folders; `&shot=selftest` and `&shot=add` open views for screenshots.
3. **Virtual DSM**: the real chain against DSM. Synology's licence allows DSM only on Synology hardware; see Syno Companion's [test environment](https://github.com/SoulInfernoDE/synology-companion-app/blob/main/docs/test-environment.md) for Virtual Machine Manager and the `vdsm/virtual-dsm` container.

## Against a Virtual DSM

Put the test VM's data in `.env.vdsm` (ignored by git):

```sh
VDSM_HOST=127.0.0.1
VDSM_HTTPS_PORT=25001
VDSM_SSH_PORT=25022
VDSM_USER=mountuser
VDSM_PASSWORD=…
VDSM_SHARE=mounttest
```

On the VM: a shared folder `mounttest`, an account with read/write access to it, SFTP and the user home service on (the app can switch them on with an administrator). For the permission test, a folder `posix` inside the share without ACL (`sudo synoacltool -del /volume1/mounttest/posix`, owned by the test account).

```bash
set -a; . ./.env.vdsm; set +a
cargo run -p syno-ssh --example vdsm-key      # key install, key login, key removal
cargo run -p syno-mount --example vdsm    # + DSM, shares, sshfs, self-test, speed test
cargo run -p syno-mount --example key-check   # key install as the assistant does it, and why a key is refused
cargo run -p syno-dsm --example key-login     # "Prepare key login" for VDSM_USER (needs VDSM_ADMIN_USER/_PASSWORD)
```

Both run in temporary folders and remove the key from the NAS at the end. The `systemd` example sets up a real service on this computer and leaves it in place until `… --example systemd remove`.

## Results with DSM 7.2.2

|                               | Regular account (no DSM access)                                                        | Administrator                                 |
| ----------------------------- | -------------------------------------------------------------------------------------- | --------------------------------------------- |
| DSM sign-in                   | refused (402), assistant goes on by SFTP                                               | works, services readable                      |
| SFTP view                     | chroot: `/home`, `/<share>`                                                            | chroot as well: `/home`, `/homes`, `/<share>` |
| Key install                   | works; DSM shows the home as 777 over SFTP and refuses `chmod`, the real folder is 711 | works                                         |
| `chmod` in share with ACL     | ignored (777)                                                                          | ignored (777)                                 |
| `chmod` in folder without ACL | works                                                                                  | works                                         |
| Symlinks                      | created, not read back                                                                 | created, not read back                        |
| xattr                         | not supported                                                                          | not supported                                 |

## Under systemd, without changing the computer

`… --example systemd prepare` with `HOME` and `XDG_*` pointing to a test folder writes the folder list, key and owner maps there. `systemd-run --user --unit=syno-mount-<id> --setenv=…` then runs `syno-mount run-mount <id>` and `syno-mount agent` as transient services, which are gone when stopped.

| Test                | Result                                                                                               |
| ------------------- | ---------------------------------------------------------------------------------------------------- |
| Mount under systemd | mounted after 2 s                                                                                    |
| `kill -9` of sshfs  | back after 6 s with one restart (after a fix: `run-mount` releases the dead mount first)             |
| NAS switched off    | the agent noticed within 14 s, released the mount and restarted the service, which waits for the NAS |
| NAS back            | mounted again 18 s after the VM started, without any action                                          |

Not tested yet: suspend and resume of a real computer, and a real network change.

## Behind a reverse proxy

nginx in a container in front of the Virtual DSM stands in for the reverse proxy; the configuration is the one in [https-tunnel.md](https-tunnel.md), with `proxy_pass 127.0.0.1:25022` and a self-signed certificate. The probe and the host key read through the tunnel:

```bash
cargo run -p syno-ssh --example tunnel-probe -- 127.0.0.1 28443 proxy.test.local pin=SHA256:…
```

For a mount, `… --example systemd prepare` writes the folder, then its internet address gets a `tunnel` in `mounts.json` and the LAN port a closed one, so that `syno-mount run-mount <id>` takes the tunnel. Results: [https-tunnel.md](https-tunnel.md#phase-1-tested).

## Against a real NAS (DSM 7.4.1)

A Synology NAS with DSM 7.4.1, encrypted shared folders on SSD, and an account made for the test (not an administrator, two-step verification enforced). Linux client: the build of this branch in an LXC container; Windows client: the installer from the release workflow on a Windows 11 VM. The password came from auth-switch (`auth-switch exec --synology --password`) and was only used to install the key.

| Test                                                            | Result                                                                                  |
| --------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Password over SFTP with two-step verification enforced          | accepted; the key is installed                                                          |
| Key login, home folder with Windows ACL                         | refused (`777` on `.ssh` and `authorized_keys`); works after the commands in the README |
| Linux, LAN                                                      | mounted after 0.27 s, latency 2.4 ms; self-test as for an ACL share                     |
| Linux, write and read 128–256 MiB                               | 98 MiB/s write, 105 MiB/s read; direct SFTP 92/97 MiB/s                                 |
| Linux, internet name over IPv6                                  | mounted after 0.3 s, route "internet", DNS "plain" shown                                |
| Linux, internet address over IPv4 without port forwarding       | not mounted; journal: `…:5009: No route to host`                                        |
| Linux, "Through HTTPS" to a web server without the stream block | not mounted; the proxy refuses the ALPN protocol                                        |
| Windows, LAN                                                    | `Z:` after 2.6 s; self-test; 32 MiB identical                                           |
| Windows, internet name over IPv6                                | `Z:` after 1.8 s, route "internet"                                                      |
| Windows, stop                                                   | `Z:` gone after 1.5 s                                                                   |

Found on the way: a Windows self-test could leave its empty test folder behind while rclone was still uploading; the cleanup now tries again for a moment.

## Home folder with Windows ACL (Virtual DSM 7.4.1)

The Virtual DSM's homes show 711 and take the key. To reproduce the real NAS, a fresh regular account `acltest` got an ACL entry that lets everyone write (`sudo synoacltool -add /var/services/homes/acltest everyone:*:allow:rwxpdDaARWc--:fd--`), after which the home, `.ssh` and `authorized_keys` show 777.

| Test                                      | Result                                                                           |
| ----------------------------------------- | -------------------------------------------------------------------------------- |
| `key-check` with the ACL                  | `key-refused-acl-home`, detail "home 777, .ssh 777, authorized_keys 777"         |
| `key-login` (Task Scheduler as root)      | `Done`; home 755, `.ssh` 700, `authorized_keys` 600, no ACL; no task left        |
| `key-check` afterwards                    | key login works                                                                  |
| `key-login` again on the prepared account | `Done` (`synoacltool -del` on a path without ACL fails, which the script allows) |
| Wrong administrator password              | DSM error 3121, shown as "Account or password is wrong"; nothing created         |
| Account without home folder               | `NoHome` (exit code 10)                                                          |

## Windows

Tested on a Windows 11 VM (build 26200) with WinFsp and rclone v1.75.2 from winget and the installer built by the release workflow (`-setup.exe /S`). The VM reaches the Virtual DSM through an SSH reverse tunnel (`ssh -R 25022:127.0.0.1:25022 <vm>`), so nothing on the network changes. The folder list, key and `known_hosts` were written as the app writes them; `syno-mount.exe status` and `syno-mount.exe selftest <id>` read the result without a window.

| Test                         | Result                                                                                                                                 |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Agent started, folder wanted | `Z:` after 3 s, route LAN, latency 1.4 ms                                                                                              |
| Self-test                    | write and read, times, rename over a file work; locks only on this computer                                                            |
| 32 MiB written and read back | identical                                                                                                                              |
| Folder stopped               | `Z:` gone after 1 s                                                                                                                    |
| Found and fixed              | DSM offers several host key types and rclone took another one ("key mismatch"); the pinned type is now passed as `host_key_algorithms` |

Not tested yet on Windows: the assistant and the administrator area in the window, autostart at sign-in, suspend and network changes.
