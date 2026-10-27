# Oberfläche von Syno Mount, deutsche Übersetzung. Basis ist Englisch.

-app-name = Syno Mount
-dsm = DSM

unofficial = Unabhängiges Community-Projekt, nicht mit Synology Inc. verbunden.

## Navigation

nav-label = Bereiche
nav-mounts = Freigaben
nav-add = Freigaben hinzufügen
nav-system = Dieser Computer
nav-settings = Einstellungen

## Freigaben

mounts-title = Freigaben
mounts-subtitle = Per SSHFS eingebunden: verschlüsselt, mit Unix-Rechten, im LAN und über das Internet.
mounts-empty-title = Noch keine Freigaben
mounts-empty-text = Richte die erste ein: NAS finden, einmal anmelden, Ordner wählen. { -app-name } erzeugt einen SSH-Schlüssel für diesen Computer, es wird kein Passwort gespeichert.
mounts-add = Freigaben hinzufügen
mounts-preview = Das ist die Browser-Vorschau. Starte die App, um Freigaben einzubinden; mit ?mock in der Adresse zeigt die Vorschau Beispieldaten.
mounts-system-problem = Dieser Computer kann noch nicht per SSHFS einbinden.
mounts-system-open = Zeigen, was fehlt

state-mounted = Eingebunden
state-stale = Antwortet nicht
state-connecting = Verbindet …
state-stopped = Nicht eingebunden
state-failed = Fehlgeschlagen
route-lan = LAN
route-internet = Internet
mount-latency = { $value } Latenz
mount-on = { $share } auf { $nas }
mount-mount = Einbinden
mount-unmount = Trennen
mount-open = Ordner öffnen
mount-more = Mehr
mount-selftest = Selbsttest
mount-speed = Geschwindigkeit
mount-settings = Einstellungen
mount-command = Befehl
mount-remove = Entfernen
mount-more-shares = Weitere Ordner von { $nas }
mount-log = Letzte Meldungen des Dienstes
mount-busy = Arbeitet …
mount-stale-hint = Die Verbindung ist weg. Der Agent bindet innerhalb einer halben Minute neu ein; oder jetzt trennen und neu einbinden.

## Einstellungen einer Freigabe

settings-mount-title = Einstellungen für { $share }
settings-at-login = Bei der Anmeldung einbinden
settings-at-login-hint = Ein systemd-User-Dienst bindet den Ordner ein, wenn du dich anmeldest, und nach dem Standby.
settings-bookmark = Im Dateimanager zeigen
settings-bookmark-hint = Fügt den Ordner der Seitenleiste von Nemo, Nautilus und anderen GTK-Dateimanagern hinzu.
settings-internet = Internet-Adresse
settings-internet-hint = DDNS-Name oder öffentliche Adresse mit dem weitergeleiteten SSH-Port, z. B. nas.example.org:2222. Wird genutzt, wenn die LAN-Adresse nicht antwortet. QuickConnect leitet kein SSH weiter.
settings-internet-placeholder = nas.example.org:2222
settings-internet-invalid = Das ist keine gültige Adresse.
settings-cipher = Prozessor des NAS
settings-cipher-x86 = Intel oder AMD (AES-GCM zuerst)
settings-cipher-arm = ARM (ChaCha20 zuerst)
settings-cipher-unknown = Unbekannt (AES-GCM zuerst)
settings-connections = Parallele Verbindungen
settings-connections-hint = Mehrere SSH-Verbindungen beschleunigen viele gleichzeitige Übertragungen. Braucht sshfs 3.7 oder neuer.
settings-compress = Über das Internet komprimieren
settings-compress-hint = Hilft bei langsamem Upload, kostet Rechenzeit. Im LAN nie aktiv.
settings-cache = Ordnerlisten
settings-cache-normal = 20 Sekunden zwischenspeichern (schneller)
settings-cache-fresh = Immer aktuell (mehrere Computer ändern denselben Ordner)
settings-save = Speichern
settings-saved = Gespeichert. Ein eingebundener Ordner verbindet sich mit den neuen Einstellungen neu.
settings-cancel = Abbrechen

## Entfernen

remove-title = { $share } entfernen?
remove-text = Der Ordner wird getrennt; sein Dienst, Lesezeichen und seine Einstellungen auf diesem Computer werden entfernt. Die Dateien auf dem NAS bleiben, wie sie sind.
remove-key = Auch den Schlüssel dieses Computers vom NAS entfernen
remove-key-hint = Nur wenn kein anderer Ordner dieses NAS ihn nutzt. Danach kann sich dieser Computer nicht mehr ohne Passwort anmelden.
remove-confirm = Entfernen
remove-cancel = Abbrechen

## Befehl

command-title = Der Befehl hinter { $share }
command-text = Der Dienst startet sshfs mit diesen Optionen (über die LAN-Adresse). Sonst wird auf diesem Computer nichts geändert.
command-copy = Kopieren
command-copied = Kopiert
command-close = Schließen

## Selbsttest

selftest-title = Was { $share } kann
selftest-running = Teste in einem temporären Ordner auf dem NAS …
selftest-intro = Echt ausprobiert in einem temporären Ordner, der danach wieder entfernt wird.
selftest-works = Funktioniert
selftest-limited = Eingeschränkt
selftest-fails = Funktioniert nicht
selftest-close = Schließen
selftest-again = Erneut testen
check-write-read = Dateien schreiben und lesen
check-write-read-fails = Dateien kommen nicht so zurück, wie sie geschrieben wurden.
check-chmod = Unix-Rechte (chmod)
check-chmod-works = Rechte bleiben so, wie sie gesetzt wurden.
check-chmod-fails = DSM ignoriert chmod in diesem Ordner, weil er Windows-ACLs nutzt (der Standard von DSM). Programme, die auf Rechte angewiesen sind, können hier scheitern. Ordner ohne ACL behalten Unix-Rechte, siehe README.
check-owner = Eigene Dateien gehören dir
check-owner-limited = Dateien zeigen eine Nummer statt deines Nutzers. Starte den Assistenten erneut, um den Besitzer zuzuordnen.
check-mtime = Änderungszeiten
check-rename-over = Datei durch Umbenennen ersetzen (so speichern Editoren)
check-symlink = Symbolische Links
check-symlink-limited = Links werden angelegt, aber DSMs SFTP-Server liest sie nicht zurück. Programme, die Links auf dem NAS folgen, können scheitern.
check-hardlink = Harte Links
check-xattr = Erweiterte Attribute (xattr)
check-xattr-fails = SFTP überträgt keine erweiterten Attribute. Programme, die dort Metadaten ablegen (manche Spiele, macOS-Tags, SELinux-Labels), verlieren sie.
check-lock = Dateisperren
check-lock-limited = Sperren gelten nur auf diesem Computer; ein anderer Computer sieht sie nicht. Öffne dieselbe Datenbank oder denselben Spielstand nicht von zwei Computern gleichzeitig.
check-large-file = Dateien größer als 4 GB

## Geschwindigkeitstest

speed-title = Geschwindigkeit von { $share }
speed-running = Schreibe und lese { $size } …
speed-write = Schreiben
speed-read = Lesen
speed-small = Kleine Dateien
speed-small-value = { $count } pro Sekunde
speed-note = Das Lesen kann teilweise aus dem Zwischenspeicher dieses Computers kommen. Vergleiche Einstellungen mit derselben Größe.
speed-size = Testgröße
speed-start = Starten
speed-close = Schließen

## Freigaben hinzufügen (Assistent)

add-title = Freigaben hinzufügen
add-step-nas = NAS
add-step-sign-in = Anmelden
add-step-services = Dienste
add-step-key = Schlüssel
add-step-folders = Ordner
add-step-done = Fertig
add-saved = Deine NAS
add-found = In deinem Netzwerk
add-searching = Suche im lokalen Netzwerk …
add-search-again = Erneut suchen
add-none-found = Kein NAS hat geantwortet. Eine Firewall auf diesem Computer kann die Suche blockieren (UDP-Port 9999 und Bonjour). Die Adresse kannst du immer eingeben.
add-choose = Wählen
add-forget = Aus der Liste entfernen
add-manual = Adresse eingeben
add-manual-placeholder = IP-Adresse oder Hostname, z. B. 192.168.1.20 oder nas.local:5001
add-manual-invalid = Das ist keine gültige Adresse.
add-cancel = Abbrechen
add-back = Zurück
add-next = Weiter

signin-title = Bei { $name } anmelden
signin-address = { $host }, Port { $port }
signin-text = { -app-name } meldet sich einmal bei { -dsm } an, um die Dienste zu prüfen und deine Ordner aufzulisten. Das Passwort wird nicht gespeichert.
signin-account = Konto
signin-password = Passwort
signin-submit = Anmelden
signin-busy = Melde an …
signin-otp-title = Zweistufige Bestätigung
signin-otp-text = Gib den Code aus deiner Authenticator-App ein.
signin-otp-code = Code
signin-otp-submit = Bestätigen
signin-cert-title = Unbekanntes Zertifikat
signin-cert-text = Das NAS nutzt ein Zertifikat, das dieser Computer nicht kennt, meist das eigene von DSM. Vergleiche den Fingerabdruck mit Systemsteuerung › Sicherheit › Zertifikat, bevor du ihm vertraust.
signin-cert-changed-title = Das Zertifikat hat sich geändert
signin-cert-changed-text = Dieses NAS hat vorher ein anderes Zertifikat gezeigt. Wenn du es nicht erneuert hast, sitzt vielleicht jemand dazwischen. Vertraue ihm nur, wenn du weißt, warum es sich geändert hat.
signin-cert-fingerprint = SHA-256-Fingerabdruck
signin-cert-trust = Vertrauen und weiter
signin-cert-trust-changed = Dem neuen Zertifikat vertrauen
signin-ssh-only = Ohne { -dsm }-Anmeldung
signin-ssh-only-text = Für Konten, die { -dsm } nicht nutzen dürfen: { -app-name } arbeitet dann nur über SFTP. Die Dienste können dann nicht geprüft werden.
signin-not-allowed = { -dsm } lässt dieses Konto nicht anmelden. SFTP kann trotzdem gehen; weiter ohne { -dsm }.
signin-ssh-port = SSH-Port
signin-name = Name des NAS
signin-continue = Weiter

services-title = Dienste auf { $name }
services-text = SSHFS braucht SFTP und für den Schlüssel einen Home-Ordner pro Nutzer.
services-sftp = SFTP (Systemsteuerung › Dateidienste › FTP › SFTP)
services-home = Benutzer-Home (Systemsteuerung › Benutzer und Gruppe › Erweitert)
services-on = An
services-off = Aus
services-port = SFTP-Port { $port }
services-enable = Einschalten, was aus ist
services-enable-text = Änderungen in { -dsm }: { $changes }. Sonst wird nichts geändert.
services-unknown = Nur Administratoren dürfen diese Einstellungen lesen. Wenn der nächste Schritt scheitert, bitte einen Administrator, SFTP und den Benutzer-Home-Dienst einzuschalten.
services-checking = Prüfe …

key-title = { $name } vertrauen
key-checking = Lese den Host-Key …
key-text = Bevor { -app-name } ein Passwort sendet, prüfe, dass das dein NAS ist: Vergleiche den Fingerabdruck, z. B. in einem Terminal auf dem NAS mit ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub.
key-known = Diesem Schlüssel wurde schon vertraut.
key-changed = Dieses NAS hat vorher einen anderen Schlüssel gezeigt. Mach nur weiter, wenn du weißt, warum er sich geändert hat (z. B. ein neu installiertes DSM).
key-fingerprint = { $algorithm }-Fingerabdruck
key-password = SSH-Passwort
key-password-hint = Einmal nötig, um den Schlüssel zu installieren. Leer lassen, um das { -dsm }-Passwort zu nutzen.
key-install = Vertrauen und Schlüssel installieren
key-installing = Installiere den Schlüssel …
key-explain = { -app-name } erzeugt einen ed25519-Schlüssel für diesen Computer, trägt ihn auf dem NAS in ~/.ssh/authorized_keys ein und meldet sich damit an. Es wird kein Passwort gespeichert.

folders-title = Ordner wählen
folders-text = Jeder Ordner wird an einem eigenen Ort auf diesem Computer eingebunden.
folders-chrooted = { -dsm } zeigt diesem Konto nur seine Freigaben, deshalb beginnen die Pfade bei /.
folders-mounted = Schon eingebunden
folders-local = Ordner auf diesem Computer
folders-none = Dieses Konto sieht über SFTP keine Freigaben. Prüfe seine Rechte in { -dsm } (Systemsteuerung › Gemeinsamer Ordner und Anwendungsberechtigungen › SFTP).
folders-at-login = Bei der Anmeldung und nach dem Standby einbinden
folders-bookmark = Im Dateimanager zeigen
folders-internet = Internet-Adresse (optional)
folders-create = { $count ->
        [one] Einen Ordner einbinden
       *[other] { $count } Ordner einbinden
    }
folders-creating = Binde ein …

done-title = Fertig
done-text = { $count ->
        [one] Ein Ordner ist
       *[other] { $count } Ordner sind
    } eingerichtet. Ein Selbsttest zeigt, was jeder kann.
done-finish = Zu den Freigaben

## Dieser Computer

system-title = Dieser Computer
system-text = Was SSHFS auf diesem Computer braucht.
system-sshfs = sshfs
system-sshfs-version = Version { $version }
system-sshfs-old = Version { $version }: parallele Verbindungen brauchen 3.7
system-missing = Fehlt
system-fusermount = fusermount3 (FUSE 3)
system-fuse-device = Zugriff auf /dev/fuse
system-systemd = systemd-User-Dienste
system-systemd-missing = Läuft nicht: Freigaben können nicht bei der Anmeldung starten.
system-ok = Vorhanden
system-install = Fehlendes installieren:
system-again = Erneut prüfen

## Einstellungen

app-settings-title = Einstellungen
app-settings-appearance = Darstellung
settings-skin = Stil
settings-skin-auto = Wie das System ({ $skin })
settings-scheme = Farben
settings-scheme-auto = Wie das System
settings-scheme-light = Hell
settings-scheme-dark = Dunkel
settings-language = Sprache
settings-language-auto = Wie das System
settings-version = Version { $version }
settings-license = Freie Software unter der GNU GPL 3.0 oder neuer.
settings-files = Dateien
settings-files-text = Schlüssel, known_hosts und Besitzer-Zuordnungen: ~/.local/share/syno-mount. Ordnerliste: ~/.config/syno-mount. Dienste: ~/.config/systemd/user/syno-mount-*.service.

## Fehler

error-network = Das NAS ist nicht erreichbar. Prüfe die Adresse und ob es eingeschaltet ist.
error-certificate-untrusted = Das NAS hat ein unbekanntes Zertifikat gezeigt.
error-certificate-changed = Das Zertifikat des NAS hat sich geändert.
error-wrong-credentials = Konto oder Passwort ist falsch.
error-account-disabled = Dieses Konto ist in { -dsm } deaktiviert.
error-account-not-allowed = { -dsm } lässt dieses Konto nicht anmelden.
error-otp-required = Ein Code der zweistufigen Bestätigung wird gebraucht.
error-otp-wrong = Der Code ist falsch oder abgelaufen.
error-otp-enforced = { -dsm } verlangt für dieses Konto die zweistufige Bestätigung; richte sie zuerst in { -dsm } ein.
error-ip-blocked = { -dsm } hat die Adresse dieses Computers nach Fehlversuchen gesperrt. Warte oder hebe die Sperre in Systemsteuerung › Sicherheit auf.
error-password-expired = Das Passwort ist abgelaufen; ändere es zuerst in { -dsm }.
error-sign-in-failed = Die Anmeldung ist fehlgeschlagen.
error-session-expired = Die { -dsm }-Sitzung ist beendet. Starte den Assistenten neu.
error-permission-denied = Das dürfen nur Administratoren.
error-unexpected = Etwas Unerwartetes ist passiert.
error-discovery = Die Netzwerksuche konnte nicht starten.
error-storage = Die Einstellungen konnten nicht gespeichert werden.
error-not-connected = Der Assistent wurde unterbrochen. Fang neu an.
error-not-found = Diesen Ordner gibt es nicht mehr.
error-ssh-network = Der SSH-Dienst des NAS antwortet nicht. Ist SFTP eingeschaltet und der Port richtig?
error-host-key-unknown = Dem Host-Key des NAS wurde noch nicht vertraut.
error-host-key-changed = Der Host-Key des NAS hat sich geändert. { -app-name } verbindet sich erst, wenn du dem neuen Schlüssel vertraust.
error-ssh-auth-failed = Das NAS hat die Anmeldung abgelehnt. Prüfe das Passwort und ob SFTP für dieses Konto erlaubt ist (Systemsteuerung › Anwendungsberechtigungen).
error-sftp = Das NAS hat einen Dateizugriff abgelehnt.
error-io = Eine Datei auf diesem Computer konnte nicht geschrieben werden.
error-selftest = Der Test konnte nicht laufen.
