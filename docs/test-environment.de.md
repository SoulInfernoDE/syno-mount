# Testumgebung

> Deutsche Fassung. Englisches Original: [test-environment.md](test-environment.md)

Syno Mount wird **ohne Produktiv-NAS** getestet, auf drei Ebenen.

1. **Unit-Tests** (`cargo test`): Befehlszeile, Units, Auswertung von mountinfo, Besitzer-Zuordnung, Schlüsseldateien, Bearbeiten von `authorized_keys`, Lesezeichen und der Selbsttest in einem lokalen Ordner.
2. **Browser-Vorschau** (`npm run dev`, dann `http://localhost:5175/?mock`): die ganze Oberfläche mit Beispieldaten, auch der Assistent (Passwort `demo`). `?mock=empty` startet ohne Ordner; `&shot=selftest` und `&shot=add` öffnen Ansichten für Screenshots.
3. **Virtual DSM**: die echte Kette gegen DSM. Synologys Lizenz erlaubt DSM nur auf Synology-Hardware; zu Virtual Machine Manager und dem Container `vdsm/virtual-dsm` siehe die [Testumgebung](https://github.com/SoulInfernoDE/synology-companion-app/blob/main/docs/test-environment.de.md) von Syno Companion.

## Gegen ein Virtual DSM

Die Daten der Test-VM kommen in `.env.vdsm` (von git ignoriert):

```sh
VDSM_HOST=127.0.0.1
VDSM_HTTPS_PORT=25001
VDSM_SSH_PORT=25022
VDSM_USER=mountuser
VDSM_PASSWORD=…
VDSM_SHARE=mounttest
```

Auf der VM: eine Freigabe `mounttest`, ein Konto mit Lese- und Schreibrecht darauf, SFTP und Benutzer-Home an (die App kann beides mit einem Administrator einschalten). Für den Rechte-Test ein Ordner `posix` in der Freigabe ohne ACL (`sudo synoacltool -del /volume1/mounttest/posix`, Besitzer ist das Testkonto).

```bash
set -a; . ./.env.vdsm; set +a
cargo run -p syno-ssh --example vdsm-key      # Schlüssel installieren, damit anmelden, entfernen
cargo run -p syno-mount --example vdsm    # + DSM, Freigaben, sshfs, Selbsttest, Geschwindigkeit
```

Beide laufen in temporären Ordnern und entfernen den Schlüssel am Ende vom NAS. Das Beispiel `systemd` richtet einen echten Dienst auf diesem Computer ein und lässt ihn stehen, bis `… --example systemd remove` läuft.

## Ergebnisse mit DSM 7.2.2

|                             | Normales Konto (ohne DSM-Zugang)                                                                 | Administrator                                      |
| --------------------------- | ------------------------------------------------------------------------------------------------ | -------------------------------------------------- |
| DSM-Anmeldung               | abgewiesen (402), Assistent macht per SFTP weiter                                                | geht, Dienste lesbar                               |
| SFTP-Ansicht                | Chroot: `/home`, `/<freigabe>`                                                                   | ebenfalls Chroot: `/home`, `/homes`, `/<freigabe>` |
| Schlüssel installieren      | geht; DSM zeigt den Home-Ordner über SFTP als 777 und lehnt `chmod` ab, der echte Ordner hat 711 | geht                                               |
| `chmod` in Freigabe mit ACL | ignoriert (777)                                                                                  | ignoriert (777)                                    |
| `chmod` in Ordner ohne ACL  | geht                                                                                             | geht                                               |
| Symlinks                    | angelegt, nicht zurückgelesen                                                                    | angelegt, nicht zurückgelesen                      |
| xattr                       | nicht unterstützt                                                                                | nicht unterstützt                                  |

## Unter systemd, ohne den Computer zu verändern

`… --example systemd prepare` mit `HOME` und `XDG_*` in einem Testordner schreibt Ordnerliste, Schlüssel und Besitzer-Zuordnung dorthin. `systemd-run --user --unit=syno-mount-<id> --setenv=…` startet dann `syno-mount run-mount <id>` und `syno-mount agent` als transiente Dienste, die beim Stoppen verschwinden.

| Test                    | Ergebnis                                                                                               |
| ----------------------- | ------------------------------------------------------------------------------------------------------ |
| Einbinden unter systemd | nach 2 s eingebunden                                                                                   |
| `kill -9` von sshfs     | nach 6 s mit einem Neustart zurück (nach einem Fix: `run-mount` löst den toten Mount zuerst)           |
| NAS ausgeschaltet       | der Agent bemerkte es binnen 14 s, löste den Mount und startete den Dienst neu, der auf das NAS wartet |
| NAS wieder da           | 18 s nach dem Start der VM wieder eingebunden, ohne Zutun                                              |

Noch nicht getestet: Standby und Aufwachen eines echten Computers und ein echter Netzwechsel.

## Windows

Getestet auf einer Windows-11-VM (Build 26200) mit WinFsp und rclone v1.75.2 aus winget und dem Installer aus dem Release-Workflow (`-setup.exe /S`). Die VM erreicht das Virtual DSM über einen SSH-Rücktunnel (`ssh -R 25022:127.0.0.1:25022 <vm>`), am Netzwerk ändert sich also nichts. Ordnerliste, Schlüssel und `known_hosts` wurden so geschrieben, wie die App sie schreibt; `syno-mount.exe status` und `syno-mount.exe selftest <id>` lesen das Ergebnis ohne Fenster.

| Test                                 | Ergebnis                                                                                                                                                  |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Agent gestartet, Ordner gewünscht    | `Z:` nach 3 s, Weg LAN, Latenz 1,4 ms                                                                                                                     |
| Selbsttest                           | Schreiben und Lesen, Zeiten, Umbenennen über eine Datei funktionieren; Sperren nur auf diesem Computer                                                    |
| 32 MiB geschrieben und zurückgelesen | identisch                                                                                                                                                 |
| Ordner gestoppt                      | `Z:` nach 1 s weg                                                                                                                                         |
| Gefunden und behoben                 | DSM bietet mehrere Host-Key-Typen an und rclone nahm einen anderen („key mismatch“); der festgehaltene Typ wird jetzt als `host_key_algorithms` übergeben |

Unter Windows noch nicht getestet: Assistent und Adminbereich im Fenster, Autostart bei der Anmeldung, Standby und Netzwechsel.
