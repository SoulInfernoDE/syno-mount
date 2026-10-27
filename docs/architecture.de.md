# Architektur

> Deutsche Fassung. Englisches Original: [architecture.md](architecture.md)

```
┌──────────────────────────── Linux-Desktop ─────────────────────────────┐
│  Oberfläche: Svelte 5 im System-WebView (Tauri 2), Fluent-Übersetzung  │
│        │ Tauri-Befehle                                                 │
│  syno-mount-app   Befehle; außerdem `run-mount <id>` und `agent`       │
│   ├─ syno-discovery   Suche über Bonjour und Synology Assistant        │
│   ├─ syno-dsm         DSM-Web-API: Anmeldung, SFTP, Home, Freigaben    │
│   ├─ syno-ssh         ed25519-Schlüssel, known_hosts, Installation     │
│   └─ syno-mount       sshfs-Befehl, systemd-Units, Status, Selbsttest  │
│                                                                        │
│  systemd --user:  syno-mount-<id>.service → syno-mount run-mount <id>  │
│                   syno-mount-agent.service → syno-mount agent          │
└──────────────┬───────────────────────────────────┬─────────────────────┘
      HTTPS (5001), nur beim Einrichten      SFTP über SSH (22 oder weitergeleitet)
┌──────────────┴───────────────────────────────────┴─────────────────────┐
│  Synology-NAS, DSM 7.2+                                                │
└────────────────────────────────────────────────────────────────────────┘
```

Alle Crates außer `syno-mount-app` enthalten keinen Oberflächen-Code, damit Syno Companion sie übernehmen kann. `syno-dsm` und `syno-discovery` sind Kopien von `companion-dsm` und `companion-discovery` aus Companion; `syno-dsm` ergänzt die Aufrufe in `files.rs`.

## Einrichtung

1. **DSM-Anmeldung** (optional): `SYNO.API.Auth`, mit zweistufiger Bestätigung und Zertifikats-Pinning wie in Companion. Sie liest `SYNO.Core.FileServ.FTP.SFTP` und `SYNO.Core.User.Home` und schaltet beides nach Rückfrage ein (nur Administratoren dürfen das). Der Home-Dienst braucht ein Volume (`location`), sonst antwortet DSM mit 3103. Konten, die DSM abweist (Fehler 402), machen ohne DSM weiter.
2. **Host-Key**: mit russh gelesen, als SHA-256-Fingerabdruck gezeigt und unter dem Alias `syno-mount.<host>_<port>` in `~/.local/share/syno-mount/known_hosts` festgehalten. ssh nutzt ihn über `HostKeyAlias`, sodass LAN- und Internet-Adresse einen Eintrag teilen.
3. **Schlüssel**: ed25519, aus `/dev/urandom` erzeugt, mit Rechten 600 gespeichert; Kommentar `syno-mount:<konto>@<computer>`. Er wird mit dem Passwort per SFTP an `authorized_keys` angehängt; danach meldet sich die App mit dem Schlüssel an, um zu prüfen, dass sshd ihn annimmt.
4. **Ordner**: DSM 7 sperrt jedes SFTP-Konto in eine Chroot-Umgebung, deren `/` die Freigaben enthält, die das Konto öffnen darf (auch Administratoren), und der Home-Ordner ist `/home`. Angeboten wird, was SFTP zeigt; die Liste von DSM fügt nichts hinzu, was SFTP nicht öffnen kann.

Das Passwort liegt nur im Speicher, solange der Assistent offen ist.

## Einbinden

`run-mount <id>` lädt den Ordner aus `mounts.json`, probiert die LAN-Adresse und dann die Internet-Adresse (TCP, je 1,5 s, bis zu 30 s lang), notiert die Wahl in `~/.local/state/syno-mount/<id>.json` und ersetzt sich durch `sshfs -f`. Optionen:

- ssh: `IdentityFile`, `IdentitiesOnly`, `UserKnownHostsFile`, `HostKeyAlias`, `HostKeyAlgorithms`, `StrictHostKeyChecking=yes`, `BatchMode=yes`, `ServerAliveInterval=15`, `ServerAliveCountMax=3`, `Ciphers` (AES-GCM zuerst bei Intel/AMD, ChaCha20 zuerst bei ARM; Kommas für FUSE maskiert), Kompression nur über das Internet.
- sshfs: `reconnect`, `max_conns` (ab sshfs 3.7), `auto_unmount`, `fsname=syno-mount:<id>`, Cache für Ordnerlisten und `idmap=file` mit `uidfile`/`gidfile`, die die Nummern des Kontos auf dem NAS dem lokalen Nutzer und seiner Gruppe zuordnen.

Der Dienst hat `Restart=on-failure`, systemd versucht es also alle 10 Sekunden erneut, solange das NAS weg ist.

## Agent

Alle 5 Sekunden vergleicht der Agent die Zeit seit dem Start (`/proc/uptime`, zählt Standby mit) mit einer monotonen Uhr, die das nicht tut, und `/proc/net/route` mit dem letzten Stand. Nach Standby oder geänderten Routen, und ohnehin alle 30 Sekunden, prüft er jeden laufenden Ordner: Ein Mount, der nicht binnen 4 Sekunden antwortet, wird gelöst (`fusermount3 -uz`) und sein Dienst neu gestartet; ein Ordner auf der Internet-Adresse wechselt zurück, sobald das LAN wieder antwortet, und umgekehrt.

## Selbsttest

In einem temporären Ordner im Mount: 1 MiB schreiben und lesen, `chmod 640`, Besitzer, `mtime`, über eine Datei umbenennen, Symlink anlegen und zurücklesen, harter Link, `user.*`-xattr, `flock` und eine dünn belegte 5-GiB-Datei. Sperren gelten immer als eingeschränkt, weil SFTP sie nicht weitergibt.
