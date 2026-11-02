# HTTPS-Tunnel (Entwurf)

> Deutsche Fassung. Englisches Original: [https-tunnel.md](https-tunnel.md)

Stand: Entwurf, davon ist noch nichts gebaut.

## Ziel

Ein NAS, dessen einzige Tür ins Internet ein Reverse-Proxy auf Port 443 ist (zum Beispiel `synoserver.example.org`), lässt sich von überall einbinden, ohne weitergeleiteten SSH-Port und ohne Zusatzprogramme auf dem Client. Der Nutzer setzt neben der Internet-Adresse ein Häkchen „über HTTPS verbinden“; alles andere erledigt Syno Mount.

Der Tunnel transportiert Bytes und sonst nichts: Rechte, Besitzer, Zeiten, Umbenennen und Links verhalten sich genau wie im LAN, und der Selbsttest zeigt dasselbe Ergebnis. (Erweiterte Attribute bleiben weg, wie ohnehin über DSMs SFTP.)

## So funktioniert es

```text
 Client                           Reverse-Proxy :443                      NAS
 sshfs/rclone ─ ssh ─ syno-mount tunnel ══ TLS ══ nginx stream ── TCP ── sshd :2222
                      (stdio ↔ TLS, ALPN)        leitet nach ALPN oder SNI,
                                                 beendet TLS, reicht SSH weiter
```

- Syno Mount baut TLS zum Proxy auf Port 443 auf und bietet das ALPN-Protokoll `syno-mount-ssh` an.
- Der Proxy schaut sich das TLS-Hello an, ohne zu entschlüsseln (`ssl_preread`). Verbindungen mit diesem ALPN (oder einem eigenen Namen wie `ssh.example.org`) gehen an einen TLS-Serverblock, der TLS beendet und die Bytes an den SSH-Port des NAS weitergibt. Alle anderen Verbindungen gehen wie bisher an den Web-Proxy.
- Darin läuft SSH Ende-zu-Ende. Der Proxy sieht nur die Verschlüsselung von SSH selbst.

Die Weiterleitung nach ALPN ist der Standard, weil sie mit dem vorhandenen Namen und Zertifikat funktioniert: kein neuer DNS-Eintrag, kein neues Zertifikat. Ein eigener Name (SNI) ist die Wahl für Proxys, die nicht nach ALPN weiterleiten können.

## Vertrauen und Sicherheit

- **Vertrauensanker bleibt der festgehaltene SSH-Host-Key.** Er wird bei der Einrichtung im LAN festgehalten; durch den Tunnel muss derselbe Schlüssel erscheinen, sonst stoppt der Mount, genau wie heute. Ein Proxy, ein kaputtes Zertifikat oder ein Angreifer dazwischen kann nichts mitlesen oder ändern.
- **TLS wird zusätzlich geprüft:** gegen die Zertifikate des Systems (Let's Encrypt und Co.). Ein selbst signiertes Proxy-Zertifikat wird nach Bestätigung des Nutzers über seinen SHA-256-Fingerabdruck festgehalten, genau wie der Host-Key.
- **Brute-Force:** Hinter einem Proxy sieht DSM bei jeder SSH-Verbindung die Adresse des Proxys. Die automatische Blockierung von DSM würde dann den Proxy aussperren und mit ihm alle Nutzer. Deshalb:
  - die Adresse des Proxys in DSM auf die Erlaubt-Liste setzen (Systemsteuerung › Sicherheit › Schutz);
  - Versuche am Proxy begrenzen (`limit_conn` in nginx, fail2ban auf sein Protokoll, das die echte Client-Adresse enthält);
  - in Phase 3 lösen Client-Zertifikate (siehe unten) das Problem: Ohne Zertifikat erreicht niemand überhaupt sshd.
- **Angriffsfläche des NAS:** Der SSH-Port des NAS muss nur dem Proxy und dem LAN antworten (Firewall-Regel in DSM). Port 443 ist ohnehin offen.
- **Konten:** Mounts nutzen nur die Schlüssel-Anmeldung, am besten mit Konten außerhalb der Administratoren, mit SFTP und nur den nötigen Freigaben. Das richtet der Adminbereich von Syno Mount ein.

### Phase 3: Client-Zertifikate (mTLS)

Jeder Computer bekommt ein Gerätezertifikat (ECDSA P-256, von Syno Mount erzeugt, neben dem SSH-Schlüssel mit Modus 600 abgelegt). Die App zeigt seinen Fingerabdruck und exportiert es als PEM. Der Proxy lässt nur Verbindungen mit einem bekannten Zertifikat durch:

```nginx
ssl_verify_client on;
ssl_client_certificate /etc/nginx/syno-mount-devices.pem;  # one PEM per device, appended
```

Ein verlorener Laptop wird ausgesperrt, indem man sein PEM aus der Datei löscht, ohne die SSH-Schlüssel anderer Computer anzufassen.

## Leistung

Gemessen auf einem Computer (Client, nginx und Virtual DSM auf derselben Maschine), 512 MiB mit `dd`, sshfs mit `aes128-gcm` und vier Verbindungen, `openssl s_client` als Ersatz für den Tunnel:

| Weg                 | Schreiben    | Lesen        |
| ------------------- | ------------ | ------------ |
| Direkt zum SSH-Port | 640–714 MB/s | 621–639 MB/s |
| Über den TLS-Proxy  | 328 MB/s     | 320–321 MB/s |

Auf einer Maschine konkurriert die ganze Verschlüsselung um denselben Prozessor; das ist also der ungünstigste Fall. 320 MB/s sind etwa 2,6 Gbit/s, mehr als der Upload üblicher Internetanschlüsse; über das Internet bestimmen Leitung und Latenz die Geschwindigkeit, nicht der Tunnel. Der eingebaute Tunnel mit großen Puffern sollte näher an den direkten Wert kommen; Phase 1 misst das erneut, Ziel sind 80 % des direkten Werts auf einer Maschine.

Das NAS hat keine Mehrarbeit: TLS endet am Proxy.

## Teile in Syno Mount

**Konfiguration.** `Address` bekommt ein optionales `tunnel`:

```rust
pub struct Tunnel {
    /// The name in the TLS hello; the proxy's name, or a dedicated one.
    pub server_name: String,
    /// ALPN routing (default) or routing by name alone.
    pub alpn: bool,
    /// SHA-256 of a self-signed proxy certificate, pinned by the user.
    pub pinned_certificate: Option<String>,
    /// Phase 3: present the device certificate.
    pub client_certificate: bool,
}
```

Alte Konfigurationsdateien bleiben gültig (`#[serde(default)]`).

**Transport in `syno-ssh`.** Ein kleines Modul öffnet entweder TCP oder TLS (rustls mit dem ring-Backend, wie bei russh; kein aws-lc). Einrichtung, Host-Key-Abfrage und Schlüssel-Installation nutzen es über `russh::client::connect_stream`, sodass auch der Assistent durch den Tunnel funktioniert.

**`syno-mount tunnel <id> <route>`.** Ein Befehl ohne Fenster, der stdin/stdout mit TLS verbindet. Unter Linux bekommt sshfs `-o ProxyCommand=<programm> tunnel <id> internet`; Host-Key-Alias, festgehaltener Schlüsseltyp und alles andere bleiben, wie sie sind.

**Windows.** rclone nutzt sein eigenes SSH, ohne ProxyCommand. Der Agent öffnet deshalb für jeden getunnelten Mount einen Weiterleiter auf `127.0.0.1` mit zufälligem Port, solange der Mount läuft, und rclone verbindet sich dorthin. Der festgehaltene Host-Key wird vor dem Start von rclone für `[127.0.0.1]:<port>` eingetragen.

**Probe.** Bei einer getunnelten Adresse heißt „erreichbar“: TCP, TLS-Handshake und das SSH-Banner (`SSH-2.0-…`) innerhalb der Wartezeit. Die Statusseite kann dann „der Proxy antwortet nicht“, „das Zertifikat passt nicht“ und „das NAS hinter dem Proxy antwortet nicht“ unterscheiden.

**Oberfläche.** Neben der Internet-Adresse: „Direkt (SSH-Port)“ oder „Über HTTPS (Reverse-Proxy)“. Bei HTTPS ist der Port 443, der Name kommt aus der Adresse. Ein Knopf **Prüfen** verbindet sich durch den Tunnel und vergleicht den Host-Key mit dem festgehaltenen. **Proxy-Konfiguration kopieren** erzeugt den nginx-Block unten mit eingesetzten Namen und Ports, für den, der den Proxy betreibt.

## Proxy-Konfiguration (nginx)

nginx braucht das Modul `stream` mit `ssl_preread`; die offiziellen Pakete und Images haben beides. Der bisherige HTTPS-Server zieht von 443 auf einen internen Port um.

```nginx
stream {
    map $ssl_preread_alpn_protocols $route {
        ~\bsyno-mount-ssh\b  127.0.0.1:8444;   # Syno Mount
        default              127.0.0.1:8443;   # the web proxy as before
    }
    server { listen 443; listen [::]:443; ssl_preread on; proxy_pass $route; }

    server {
        listen 127.0.0.1:8444 ssl;
        ssl_certificate     /etc/letsencrypt/live/synoserver.example.org/fullchain.pem;
        ssl_certificate_key /etc/letsencrypt/live/synoserver.example.org/privkey.pem;
        ssl_alpn syno-mount-ssh;
        proxy_pass 192.168.1.20:2222;           # SSH port of the NAS
    }
}
```

Die Dokumentation bekommt dasselbe für Traefik (TCP-Router mit `HostSNI` oder ALPN) und HAProxy (`req.ssl_alpn`). Der Web-Proxy hinter 8443 sieht die Client-Adressen über das PROXY-Protokoll, wenn es an beiden Enden eingeschaltet ist.

**Nicht auf dem nginx von DSM.** Web Station und der Reverse-Proxy von DSM verwalten nur HTTP-Hosts. Ein `stream`-Block gehört nicht zur Konfiguration von DSM, wird bei Updates überschrieben, und ob das nginx von DSM das Modul mitbringt, ist nicht gesichert. Der Proxy gehört auf einen eigenen Host oder Container.

## Was nicht gewählt wurde

- **SSH ohne TLS auf Port 443** (nginx unterscheidet TLS und SSH): funktioniert heute ohne Änderung an Syno Mount und wird so dokumentiert, aber SSH ist auf 443 für Scanner offen, und manche Hotel- und Firmennetze lassen nur echtes TLS durch.
- **SSH über WebSocket:** käme auch durch reine HTTP-Proxys, braucht aber eine Brücke von WebSocket auf TCP neben dem NAS. Bleibt für später, zusammen mit Cloudflare Tunnel.
- **Tailscale oder WireGuard:** Hier ist gar nichts zum Internet offen; das bleibt der Rat für alle, die keinen Port öffnen wollen. Funktioniert heute schon: die Tailscale-Adresse als Internet-Adresse eintragen.

## Plan

1. Transport mit TLS und ALPN, `syno-mount tunnel`, sshfs unter Linux, Probe, Oberfläche, nginx-Dokumentation. Test: nginx-Container vor Virtual DSM (wie bei der Messung), falscher Name, falsches Zertifikat, Proxy aus, NAS hinter dem Proxy aus; Geschwindigkeit gegen direkt.
2. Windows-Weiterleiter für rclone; Test auf der Windows-11-VM.
3. Gerätezertifikate (mTLS) mit Export und Fingerabdruck.
4. Proxy-Konfiguration zum Kopieren, für nginx, Traefik und HAProxy.

Neue Crates: `rustls` (ring), `tokio-rustls`, `rustls-platform-verifier`, später `rcgen`; alle Apache-2.0, MIT oder ISC, was `deny.toml` erlaubt.

Offene Punkte: Die Weiterleitung nach Namen ist getestet (siehe Messung); die nach ALPN wird in Phase 1 geprüft, ebenso ob sie mit anderen verbreiteten Frontends funktioniert (Nginx Proxy Manager, Caddy mit dem layer4-Modul). Außerdem, wie weit der eingebaute Tunnel auf einer Maschine kommt.
