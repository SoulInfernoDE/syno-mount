# HTTPS tunnel (design draft)

> English original. German version: [https-tunnel.de.md](https-tunnel.de.md)

Status: draft, nothing of it is built yet.

## Goal

A NAS whose only door to the internet is a reverse proxy on port 443 (for example `synoserver.example.org`) can be mounted from anywhere, without a forwarded SSH port and without extra programs on the client. The user ticks "connect through HTTPS" next to the internet address; everything else is done by Syno Mount.

The tunnel carries bytes and nothing else: permissions, owners, times, renaming and links behave exactly as on the LAN, and the self-test shows the same result. (Extended attributes stay unavailable, as they are over DSM's SFTP anyway.)

## How it works

```text
 Client                           Reverse proxy :443                      NAS
 sshfs/rclone ─ ssh ─ syno-mount tunnel ══ TLS ══ nginx stream ── TCP ── sshd :2222
                      (stdio ↔ TLS, ALPN)        routes by ALPN or SNI,
                                                 ends TLS, passes SSH on
```

- Syno Mount opens TLS to the proxy on port 443 and offers the ALPN protocol `syno-mount-ssh`.
- The proxy looks at the TLS hello without decrypting (`ssl_preread`). Connections with that ALPN (or a dedicated name such as `ssh.example.org`) go to a TLS server block that ends TLS and passes the bytes to the SSH port of the NAS. All other connections go to the web proxy as before.
- SSH runs inside, end to end. The proxy sees only SSH's own encryption.

Routing by ALPN is the default because it works with the existing name and certificate: no new DNS entry and no new certificate. A separate name (SNI) is the choice for proxies that cannot route by ALPN.

## Trust and security

- **The trust anchor stays the pinned SSH host key.** It is pinned during setup on the LAN; through the tunnel the same key must appear, or the mount stops, exactly as today. A proxy, a broken certificate or a man in the middle cannot read or change anything.
- **TLS is checked as well:** against the system's certificates (Let's Encrypt and the like). A self-signed proxy certificate is pinned by its SHA-256 fingerprint after the user confirms it, in the same way as the host key.
- **Brute force:** behind a proxy, DSM sees the proxy's address for every SSH connection. DSM's auto block would then lock out the proxy and with it every user. So:
  - put the proxy's address on DSM's allow list (Control Panel › Security › Protection);
  - limit attempts on the proxy (`limit_conn` in nginx, fail2ban on its log, which has the real client address);
  - in phase 3, client certificates (below) remove the problem: without a certificate no one reaches sshd at all.
- **Exposure of the NAS:** the SSH port of the NAS only needs to answer the proxy and the LAN (DSM firewall rule). Port 443 is open already.
- **Accounts:** mounts use key login only, and ideally accounts outside the administrators, with SFTP and only the shared folders they need. The administrator area of Syno Mount sets this up.

### Phase 3: client certificates (mTLS)

Each computer gets a device certificate (ECDSA P-256, created by Syno Mount, stored next to the SSH key with mode 600). The app shows its fingerprint and exports it as PEM. The proxy only lets through connections with a known certificate:

```nginx
ssl_verify_client on;
ssl_client_certificate /etc/nginx/syno-mount-devices.pem;  # one PEM per device, appended
```

A lost laptop is shut out by deleting its PEM from the file, without touching the SSH keys of other computers.

## Performance

Measured on one computer (client, nginx and Virtual DSM all on the same machine), 512 MiB with `dd`, sshfs with `aes128-gcm` and four connections, `openssl s_client` as a stand-in for the tunnel:

| Way                    | Write        | Read         |
| ---------------------- | ------------ | ------------ |
| Direct to the SSH port | 640–714 MB/s | 621–639 MB/s |
| Through the TLS proxy  | 328 MB/s     | 320–321 MB/s |

On one machine all the encryption competes for the same processor, so this is the worst case. 320 MB/s are about 2.6 Gbit/s, more than the upload of common internet lines; over the internet the line and the latency set the speed, not the tunnel. The built-in tunnel with large buffers should come closer to the direct value; phase 1 measures it again, aiming at 80 % of direct on one machine.

The NAS does no extra work: TLS ends at the proxy.

## Parts in Syno Mount

**Configuration.** `Address` gets an optional `tunnel`:

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

Old configuration files stay valid (`#[serde(default)]`).

**Transport in `syno-ssh`.** A small module opens either TCP or TLS (rustls with the ring backend, as with russh; no aws-lc). Setup, host key scan and the key installation use it through `russh::client::connect_stream`, so the assistant works through the tunnel too.

**`syno-mount tunnel <id> <route>`.** A headless command that connects stdin/stdout to TLS. On Linux sshfs gets `-o ProxyCommand=<program> tunnel <id> internet`; host key alias, pinned key type and everything else stay as they are.

**Windows.** rclone uses its own SSH, without ProxyCommand. The agent therefore opens a forwarder for each tunnelled mount on `127.0.0.1` with a random port, for as long as the mount runs, and rclone connects to it. The pinned host key is written for `[127.0.0.1]:<port>` before rclone starts.

**Probe.** For a tunnelled address, "reachable" means: TCP, TLS handshake and the SSH banner (`SSH-2.0-…`) within the timeout. The status page can then tell apart "the proxy does not answer", "the certificate does not match" and "the NAS behind the proxy does not answer".

**Interface.** Next to the internet address: "Direct (SSH port)" or "Through HTTPS (reverse proxy)". With HTTPS, the port is 443 and the name is taken from the address. A **Check** button connects through the tunnel and compares the host key with the pinned one. **Copy proxy configuration** produces the nginx block below with the names and ports filled in, for whoever runs the proxy.

## Proxy configuration (nginx)

nginx needs the `stream` module with `ssl_preread`; the official packages and images have both. The existing HTTPS server moves from 443 to an internal port.

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

The documentation will carry the same for Traefik (TCP router with `HostSNI` or ALPN) and HAProxy (`req.ssl_alpn`). The web proxy behind 8443 sees the client addresses through the PROXY protocol if it is switched on at both ends.

**Not on DSM's own nginx.** Web Station and DSM's reverse proxy only manage HTTP hosts. A `stream` block is not part of DSM's configuration, is overwritten by updates, and whether DSM's nginx has the module is not assured. The proxy belongs on its own host or container.

## What was not chosen

- **SSH without TLS on port 443** (nginx tells TLS and SSH apart): works today without any change to Syno Mount and is documented as such, but SSH is open to scanners on 443, and some hotel and company networks only let real TLS through.
- **SSH over WebSocket:** would also pass HTTP-only proxies, but needs a bridge from WebSocket to TCP next to the NAS. Kept for later, together with Cloudflare Tunnel.
- **Tailscale or WireGuard:** nothing is open to the internet at all; this stays the advice for anyone who does not want to open a port. It works today: enter the Tailscale address as the internet address.

## Plan

1. Transport with TLS and ALPN, `syno-mount tunnel`, sshfs on Linux, probe, interface, nginx documentation. Test: nginx container in front of Virtual DSM (as in the measurement), wrong name, wrong certificate, proxy down, NAS behind the proxy down; speed against direct.
2. Windows forwarder for rclone; test on the Windows 11 VM.
3. Device certificates (mTLS) with export and fingerprint.
4. Proxy configuration to copy, for nginx, Traefik and HAProxy.

New crates: `rustls` (ring), `tokio-rustls`, `rustls-platform-verifier`, later `rcgen`; all Apache-2.0, MIT or ISC, which `deny.toml` allows.

Open points: routing by name is tested (see the measurement); routing by ALPN is checked in phase 1, as is whether it works with other common front ends (Nginx Proxy Manager, Caddy with the layer4 module). And how far the built-in tunnel gets on one machine.
