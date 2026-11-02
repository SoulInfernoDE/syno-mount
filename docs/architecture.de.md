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

Eine Internet-Adresse kann **hinter einem Reverse-Proxy** liegen (`tunnel` in `mounts.json`). Dann heißt „erreichbar“: TLS zum Proxy und das SSH-Banner des NAS dahinter, und sshfs bekommt `ProxyCommand='<programm>' tunnel <id>`: `syno-mount tunnel` öffnet TLS mit dem ALPN-Protokoll `syno-mount-ssh` (rustls, ring) und verbindet es mit Standardein- und -ausgabe von ssh. Alles andere, auch der festgehaltene Host-Key, bleibt, wie es ist. Siehe [https-tunnel.de.md](https-tunnel.de.md).

## Agent

Alle 5 Sekunden vergleicht der Agent die Zeit seit dem Start (`/proc/uptime`, zählt Standby mit) mit einer monotonen Uhr, die das nicht tut, und `/proc/net/route` mit dem letzten Stand. Nach Standby oder geänderten Routen, und ohnehin alle 30 Sekunden, prüft er jeden laufenden Ordner: Ein Mount, der nicht binnen 4 Sekunden antwortet, wird gelöst (`fusermount3 -uz`) und sein Dienst neu gestartet; ein Ordner auf der Internet-Adresse wechselt zurück, sobald das LAN wieder antwortet, und umgekehrt.

## Selbsttest

In einem temporären Ordner im Mount: 1 MiB schreiben und lesen, `chmod 640`, Besitzer, `mtime`, über eine Datei umbenennen, Symlink anlegen und zurücklesen, harter Link, `user.*`-xattr, `flock` und eine dünn belegte 5-GiB-Datei. Sperren gelten immer als eingeschränkt, weil SFTP sie nicht weitergibt.

## Adminbereich

Eine eigene DSM-Anmeldung, die ein Administrator sein muss. Sie liest `SYNO.Core.User` (Konten), `SYNO.Core.Group.Member` (Mitglieder von „administrators“), `SYNO.Core.AppPriv.Rule` für die Anwendung `SYNO.SFTP` (die Regel für alle und Regeln pro Konto), `SYNO.Core.Share` und pro Freigabe `SYNO.Core.Share.Permission`. Änderungen werden in der Oberfläche gesammelt und erst gesendet, wenn der Nutzer die Liste bestätigt hat: Eine SFTP-Regel pro Konto wird gesetzt (`set`) oder entfernt (`delete`, zurück zur Regel für alle), und die eigenen Rechte der Konten auf eine Freigabe werden in einem `set`-Aufruf geschrieben. „Angepasste“ Rechte aus DSMs ACL-Editor werden gezeigt, aber nicht geändert.

Die Anmeldung nutzt den Sitzungsnamen `FileStation`: DSM weist Konten außerhalb der Administratoren bei unbekannten Sitzungsnamen mit Fehler 402 ab, `FileStation` dagegen ist für jedes Konto offen und erlaubt trotzdem die Admin-APIs.

## Windows

Windows bindet mit [rclone](https://rclone.org) über [WinFsp](https://winfsp.dev) ein statt mit sshfs. SSHFS-Win wurde geprüft und verworfen: Die letzte Version stammt von 2021. Der Rest ist gemeinsam: Assistent, Schlüssel, festgehaltener Host-Key, Adminbereich, Selbsttest.

- **Befehl:** `rclone mount :sftp,host=…,port=…,user=…,key_file=…,known_hosts_file=…,ciphers=…,shell_type=none,disable_hashcheck=true,set_modtime=true:/<freigabe> Z: --network-mode --vfs-cache-mode writes …`. Es wird keine rclone-Konfigurationsdatei geschrieben. rclone prüft Host-Keys nach Adresse, deshalb steht der festgehaltene Schlüssel zusätzlich unter `host` und `[host]:port` für LAN- und Internet-Adresse.
- **Agent:** `syno-mount.exe agent` hält eine Sperrdatei, es läuft also einer pro Nutzer. Alle 2 Sekunden startet er für jeden Ordner, der eingebunden sein soll (eine `<id>.want`-Datei von der App oder „bei der Anmeldung einbinden“), ein rclone, startet eines neu, das beendet ist (nach 5 Sekunden) oder hängt (alle 30 Sekunden geprüft), wechselt zurück ins LAN, sobald es wieder antwortet, und beendet rclone für gestoppte oder entfernte Ordner. Die Meldungen von rclone landen in `<id>.log`; die App zeigt die letzten Zeilen, wenn ein Ordner scheitert.
- **Autostart:** Solange ein Ordner bei der Anmeldung eingebunden werden soll, startet der Wert „Syno Mount“ in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` den Agenten.
- **Selbsttest:** ohne Rechte, Besitzer, Links, xattr und die 5-GiB-Datei (rclone würde sie komplett hochladen).
