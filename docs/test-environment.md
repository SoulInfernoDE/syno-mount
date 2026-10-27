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
cargo run -p syno-ssh --example vdsm      # key install, key login, key removal
cargo run -p syno-mount --example vdsm    # + DSM, shares, sshfs, self-test, speed test
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
