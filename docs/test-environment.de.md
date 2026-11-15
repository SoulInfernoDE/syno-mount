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
cargo run -p syno-mount --example key-check   # Schlüssel installieren wie der Assistent, und warum er abgelehnt wird
cargo run -p syno-dsm --example key-login     # „Schlüssel-Login vorbereiten“ für VDSM_USER (braucht VDSM_ADMIN_USER/_PASSWORD)
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

## Hinter einem Reverse-Proxy

nginx in einem Container vor der Virtual DSM spielt den Reverse-Proxy; die Konfiguration ist die aus [https-tunnel.de.md](https-tunnel.de.md), mit `proxy_pass 127.0.0.1:25022` und einem selbst signierten Zertifikat. Probe und Host-Key durch den Tunnel:

```bash
cargo run -p syno-ssh --example tunnel-probe -- 127.0.0.1 28443 proxy.test.local pin=SHA256:…
```

Für einen Mount schreibt `… --example systemd prepare` den Ordner; danach bekommt seine Internet-Adresse in `mounts.json` ein `tunnel` und der LAN-Port einen geschlossenen, damit `syno-mount run-mount <id>` den Tunnel nimmt. Ergebnisse: [https-tunnel.de.md](https-tunnel.de.md#phase-1-getestet).

## Gegen ein echtes NAS (DSM 7.4.1)

Eine Synology-NAS mit DSM 7.4.1, verschlüsselten Freigaben auf SSD und einem eigens angelegten Konto (kein Administrator, zweistufige Bestätigung erzwungen). Linux-Client: der Build dieses Branches in einem LXC-Container; Windows-Client: der Installer aus dem Release-Workflow auf einer Windows-11-VM. Das Passwort kam aus auth-switch (`auth-switch exec --synology --password`) und wurde nur zum Installieren des Schlüssels genutzt.

| Test                                                       | Ergebnis                                                                                  |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| Passwort per SFTP bei erzwungener zweistufiger Bestätigung | angenommen; der Schlüssel wird installiert                                                |
| Schlüssel-Anmeldung, Home-Ordner mit Windows-ACL           | abgelehnt (`777` auf `.ssh` und `authorized_keys`); geht nach den Befehlen aus der README |
| Linux, LAN                                                 | eingebunden nach 0,27 s, Latenz 2,4 ms; Selbsttest wie bei einer ACL-Freigabe             |
| Linux, 128–256 MiB schreiben und lesen                     | 98 MiB/s schreiben, 105 MiB/s lesen; direktes SFTP 92/97 MiB/s                            |
| Linux, Internet-Name über IPv6                             | eingebunden nach 0,3 s, Weg „Internet“, DNS „unverschlüsselt“ angezeigt                   |
| Linux, Internet-Adresse über IPv4 ohne Portfreigabe        | nicht eingebunden; Journal: `…:5009: No route to host`                                    |
| Linux, „Über HTTPS“ zu einem Webserver ohne stream-Block   | nicht eingebunden; der Proxy lehnt das ALPN-Protokoll ab                                  |
| Windows, LAN                                               | `Z:` nach 2,6 s; Selbsttest; 32 MiB identisch                                             |
| Windows, Internet-Name über IPv6                           | `Z:` nach 1,8 s, Weg „Internet“                                                           |
| Windows, Stoppen                                           | `Z:` nach 1,5 s weg                                                                       |

Unterwegs gefunden: Ein Selbsttest unter Windows konnte seinen leeren Testordner zurücklassen, solange rclone noch hochlud; das Aufräumen versucht es jetzt kurz erneut.

## Home-Ordner mit Windows-ACL (Virtual DSM 7.4.1)

Die Home-Ordner des Virtual DSM zeigen 711 und nehmen den Schlüssel an. Um das echte NAS nachzustellen, bekam ein frisches normales Konto `acltest` einen ACL-Eintrag, der allen das Schreiben erlaubt (`sudo synoacltool -add /var/services/homes/acltest everyone:*:allow:rwxpdDaARWc--:fd--`); danach zeigen Home-Ordner, `.ssh` und `authorized_keys` 777.

| Test                                                | Ergebnis                                                                             |
| --------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `key-check` mit der ACL                             | `key-refused-acl-home`, Detail „home 777, .ssh 777, authorized_keys 777“             |
| `key-login` (Aufgabenplaner als root)               | `Done`; Home 755, `.ssh` 700, `authorized_keys` 600, keine ACL; keine Aufgabe übrig  |
| `key-check` danach                                  | Schlüssel-Anmeldung geht                                                             |
| `key-login` noch einmal auf dem vorbereiteten Konto | `Done` (`synoacltool -del` scheitert an einem Pfad ohne ACL, was das Skript zulässt) |
| Falsches Administratorpasswort                      | DSM-Fehler 3121, angezeigt als „Konto oder Passwort ist falsch“; nichts angelegt     |
| Konto ohne Home-Ordner                              | `NoHome` (Exit-Code 10)                                                              |

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
