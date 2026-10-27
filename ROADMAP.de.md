# Roadmap

> Deutsche Fassung. Englisches Original: [ROADMAP.md](ROADMAP.md)

**Syno Mount** (Arbeitstitel: Synology SSHFS Assistant) ist ein geführter Assistent, der Synology-Freigaben unter Linux per **SSHFS** einbindet: einmal eingerichtet, bei jeder Anmeldung eingebunden, nach Standby oder Netzwechsel neu verbunden, im LAN und über das Internet.

**Warum SSHFS:** Unix-Rechte und Zeitstempel bleiben erhalten, anders als bei WebDAV, das sich als instabil und eingeschränkt erwiesen hat. Ein verschlüsselter Port funktioniert über das Internet ohne VPN. NFSv4 und iSCSI folgen später als weitere Backends (siehe [Später](#später)).

**Verhältnis zu Syno Companion:** Dieselbe Logik zum Einbinden wird später in [Syno Companion](https://github.com/SoulInfernoDE/synology-companion-app) gebraucht. Diese App nutzt deshalb denselben Stack und die Crates von Companion, damit ihre Crates und Seiten ohne Neuschreiben in Companion umziehen können.

## Entscheidungen

| Thema                   | Entscheidung                                                                                                                  | Grund                                                                                                                                                                                                        |
| ----------------------- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Stack                   | Tauri 2, Svelte 5, Rust (Edition 2024), Fluent                                                                                | Wie Companion: Oberflächen-Seiten und Crates lassen sich übernehmen                                                                                                                                          |
| Plattform               | Nur Linux (x86_64, arm64)                                                                                                     | SSHFS ist nur mit Linux-FUSE zuverlässig, macFUSE und SSHFS-Win sind es nicht                                                                                                                                |
| Wiederverwendete Crates | `companion-discovery` und `companion-dsm`, kopiert als `syno-discovery` und `syno-dsm`                                        | NAS finden, Anmeldung mit 2FA und Zertifikats-Pinning funktionieren schon und sind getestet; `syno-dsm` brauchte Aufrufe, die Companion noch nicht anbietet, deshalb eine Kopie statt einer Git-Abhängigkeit |
| Neue Crates             | `syno-ssh` (Schlüssel, Host-Keys, Schlüssel installieren), `syno-mount` (Backend-Trait, SSHFS-Backend, systemd-Units, Status) | In Companion als `companion-ssh` und als Mount-Teil von v0.2 geplant                                                                                                                                         |
| Mount-Werkzeug          | `sshfs` (3.x) und `fusermount3` des Systems, nicht mitgeliefert                                                               | Distributionspakete bekommen Sicherheitsupdates; deb/rpm hängen von `sshfs` und `fuse3` ab                                                                                                                   |
| SSH-Einstellungen       | Als `-o`-Optionen pro Mount; die App hat eigene `known_hosts` und Schlüssel                                                   | Die `~/.ssh/config` des Nutzers bleibt unberührt, Entfernen ist sauber                                                                                                                                       |
| Dauerhaftigkeit         | Ein systemd-**User**-Service pro Mount                                                                                        | Kein root nötig; root nur für den optionalen Modus „bei Zugriff einbinden“ (fstab, später)                                                                                                                   |
| Pakete                  | deb, rpm, AppImage; kein Flatpak                                                                                              | FUSE-Mounts aus einer Sandbox brauchen Umwege                                                                                                                                                                |
| Lizenz                  | GPL-3.0-or-later                                                                                                              | Wie Companion                                                                                                                                                                                                |
| DSM                     | 7.2 und neuer                                                                                                                 | Die Test-VM läuft mit DSM 7.2.2-72806; Companion zielt auf 7.3+, also muss 7.2 dort geprüft werden, wo sich APIs unterscheiden                                                                               |

## Ergebnisse aus der Test-VM (DSM 7.2.2)

- SSH und SFTP sind **getrennte Schalter**: `SYNO.Core.Terminal` (SSH) und `SYNO.Core.FileServ.FTP.SFTP` (`enable`, `portnum`). Auf einem frischen DSM sind beide aus.
- Der **Benutzer-Home-Dienst** (`SYNO.Core.User.Home`) ist anfangs aus. `/var/services/homes` zeigt dann auf `@fake_home_link`, und es gibt keinen Ort für `~/.ssh/authorized_keys`. Der Assistent muss ihn einschalten (nach Rückfrage).
- sshd ist OpenSSH 8.2p1 mit `Subsystem sftp internal-sftp -u 000`: Die Umask ist 000, Rechte bleiben also so, wie der Client sie anfordert. `PasswordAuthentication yes` ist Standard.
- Die VM hat eine x86_64-CPU mit AES-NI, dort ist `aes128-gcm@openssh.com` die schnelle Verschlüsselung.

Inzwischen getestet (Details in der [Testumgebung](docs/test-environment.de.md)): DSM 7 sperrt **jedes** SFTP-Konto in eine Chroot-Umgebung, auch Administratoren; Freigaben heißen deshalb immer `/<freigabe>`, der Home-Ordner `/home`. `chmod` wird in Freigaben mit Windows-ACLs (DSM-Standard) ignoriert und funktioniert in Ordnern ohne ACL. Symbolische Links werden angelegt, aber DSMs SFTP-Server liest sie nicht zurück. Der Home-Dienst braucht beim Einschalten ein Volume (sonst Fehler 3103). Konten ohne DSM-Zugang (Fehler 402) funktionieren trotzdem über SFTP.

## v0.1 – Fundament

- [x] Repository: README (Englisch, Deutsch), LICENSE, diese Roadmap, `docs/architecture.md`, `docs/test-environment.md`, jeweils mit deutscher Fassung
- [x] Cargo-Workspace und Tauri-2-Grundgerüst, Svelte-5-Oberfläche, Fluent-Kataloge (`en` als Basis und Fallback, `de`)
- [x] Skins und Hell/Dunkel von Companion übernommen
- [x] NAS finden und anmelden über `companion-discovery` und `companion-dsm` (2FA, Zertifikats-Pinning, Schlüsselbund)
- [x] System prüfen: `sshfs` und `fusermount3` vorhanden, Version ≥ 3.7, Nutzer darf FUSE nutzen; sagen, wie Fehlendes installiert wird
- [x] CI: Build, `clippy`, Tests, Lizenzprüfung (`cargo deny`)

## v0.2 – Einrichten

- [x] Über die DSM-Web-API prüfen, ob SSH, SFTP und der Home-Dienst an sind; nach Rückfrage einschalten und zeigen, was sich ändert
- [x] SSH-Port: aus DSM lesen, vor Port 22 warnen, wenn das NAS aus dem Internet erreichbar ist
- [x] Schlüssel: einen ed25519-Schlüssel pro NAS und Konto erzeugen (ohne Passphrase, Dateirechte 600, im Datenordner der App) oder einen vorhandenen nutzen
- [x] Schlüssel installieren: eine Zeile an `~/.ssh/authorized_keys` auf dem NAS anhängen, mit Kommentar markiert, damit sie später entfernt werden kann; die Rechte setzen, die DSM erwartet. Funktioniert auch für Nutzer ohne Shell-Zugang, über SFTP
- [x] Host-Key: abrufen, SHA-256-Fingerabdruck zeigen, in den eigenen `known_hosts` der App festhalten, sobald der Nutzer ihm vertraut; ein geänderter Schlüssel stoppt jeden Mount mit Warnung
- [x] Freigaben: über `SYNO.FileStation.List list_share` auflisten; Freigaben und Mount-Ordner wählen (Standard `~/NAS/<NAS>/<Freigabe>`)
- [x] Pfade: erkennen, ob das Konto `/volume1/<Freigabe>` oder `/<Freigabe>` sieht
- [x] Von Hand in der App einbinden und trennen
- [x] **Selbsttest** nach dem Einbinden: anlegen, `chmod`, über eine vorhandene Datei umbenennen, Symlink, Hardlink, große Datei, xattr- und Lock-Prüfung. Das Ergebnis ist eine Tabelle, was geht und was nicht (xattrs und Locks gehen über SFTP nicht), damit der Nutzer vorher weiß, welche Programme Probleme bekommen
- [x] Rückgängig pro Mount und für alles: Mount stoppen, Units und Lesezeichen entfernen, Schlüsselzeile vom NAS entfernen

## v0.3 – Eingebunden bleiben

- [x] Ein systemd-User-Service pro Mount (`sshfs -f`, `Restart=on-failure`, `fusermount3 -u` beim Stoppen), bei der Anmeldung aktiv
- [x] Standardoptionen: `reconnect`, `ServerAliveInterval=15`, `ServerAliveCountMax=3`, `idmap=user`, `IdentitiesOnly=yes`, `StrictHostKeyChecking=yes`
- [x] Kleiner Hintergrund-Agent (User-Service): hört per D-Bus auf logind (`PrepareForSleep`) und NetworkManager; nach dem Aufwachen oder einem Netzwechsel löst er hängende Mounts (`fusermount3 -uz`) und bindet neu ein
- [x] Zwei Adressen pro NAS: LAN und Internet (DDNS-Name oder öffentliche Adresse); der Agent nimmt die, die antwortet, LAN zuerst. QuickConnect geht nicht, weil es kein SSH weiterleitet
- [x] Lesezeichen im Dateimanager (`~/.config/gtk-3.0/bookmarks`, genutzt von Nemo und Nautilus)
- [x] Statusseite: pro Mount verbunden oder nicht, genutzte Adresse, Latenz; Buttons zum Neu verbinden, Trennen, Im Dateimanager öffnen
- [ ] Tray-Symbol mit Status und Benachrichtigungen (optional)

## v0.4 – Schnell

- [x] Verschlüsselung nach NAS-CPU wählen (CPU-Infos aus `companion-dsm`): `aes128-gcm@openssh.com` mit AES-NI, `chacha20-poly1305@openssh.com` auf ARM
- [x] `max_conns` für parallele Übertragungen (sshfs ≥ 3.7); Kompression im LAN aus, über das Internet optional
- [x] Cache-Einstellungen für Verzeichnislisten, mit kurzer Erklärung des Kompromisses, wenn andere Rechner dieselben Dateien ändern
- [x] Geschwindigkeitstest in der App: Durchsatz (große Datei) und viele kleine Dateien, LAN und Internet, um Einstellungen zu vergleichen

## v0.5 – Veröffentlichung

- [x] Release-Builds für Linux x86_64 und arm64 (deb, rpm, AppImage); das AppImage prüft beim Start, ob `sshfs` da ist
- [x] Screenshots und README auf Englisch und Deutsch
- [ ] Kompletter Durchlauf auf einem echten Rechner mit systemd: einrichten → bei Anmeldung einbinden → Standby/Aufwachen → Netzwechsel → alles rückgängig (die Kette ohne systemd ist gegen die VM getestet, siehe [Testumgebung](docs/test-environment.de.md))
- [x] Release-Notes auf Englisch mit deutschem Abschnitt

## Später

- [ ] **NFSv4.1**-Backend: echte UID/GID und Locking im LAN; über das Internet nur per VPN (zum Beispiel Tailscale aus dem Paket-Zentrum)
- [ ] **iSCSI**-Backend: eine LUN als lokale Platte mit beliebigem Dateisystem (ext4, Btrfs, NTFS) für Spiele, Steam-Bibliotheken und Wine-Präfixe; immer nur ein Rechner, die App verhindert Doppelnutzung
- [ ] „Bei Zugriff einbinden“ über fstab mit `x-systemd.automount` (braucht root, über `pkexec`, mit Vorschau der Änderung)
- [ ] Den installierten Schlüssel auf SFTP beschränken (Recherche: welche Optionen in `authorized_keys` DSMs sshd beachtet)
- [ ] `syno-ssh` und `syno-mount` nach Syno Companion verschieben und dort nutzen (Companion-Roadmap v0.2 Mounts und v0.3 SSH)

## Offene Fragen

- Tray-Symbol: lohnt es sich noch, wo der Agent die Ordner auch ohne Fenster eingebunden hält?
- Den installierten Schlüssel auf SFTP beschränken, siehe [Später](#später).
