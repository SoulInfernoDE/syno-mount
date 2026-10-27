# Syno Mount user interface. English is the base locale.

-app-name = Syno Mount
-dsm = DSM

unofficial = Independent community project, not affiliated with Synology Inc.

## Navigation

nav-label = Sections
nav-mounts = Shared folders
nav-add = Add folders
nav-system = This computer
nav-settings = Settings

## Shared folders

mounts-title = Shared folders
mounts-subtitle = Mounted over SSHFS: encrypted, with Unix permissions, on the LAN and over the internet.
mounts-empty-title = No shared folders yet
mounts-empty-text = Set up the first one: find your NAS, sign in once, choose folders. { -app-name } creates an SSH key for this computer, so no password is stored.
mounts-add = Add folders
mounts-preview = This is the browser preview. Start the app to mount folders; with ?mock in the address the preview shows sample data.
mounts-system-problem = This computer cannot mount over SSHFS yet.
mounts-system-open = Show what is missing

state-mounted = Mounted
state-stale = Not answering
state-connecting = Connecting …
state-stopped = Not mounted
state-failed = Failed
route-lan = LAN
route-internet = Internet
mount-latency = { $value } latency
mount-on = { $share } on { $nas }
mount-mount = Mount
mount-unmount = Unmount
mount-open = Open folder
mount-more = More
mount-selftest = Self-test
mount-speed = Speed test
mount-settings = Settings
mount-command = Command
mount-remove = Remove
mount-more-shares = More folders from { $nas }
mount-log = Last messages of the service
mount-busy = Working …
mount-stale-hint = The connection is gone. The agent mounts again within half a minute; or unmount and mount now.

## Mount settings

settings-mount-title = Settings for { $share }
settings-at-login = Mount at login
settings-at-login-hint = A systemd user service mounts the folder when you sign in and after suspend.
settings-bookmark = Show in the file manager
settings-bookmark-hint = Adds the folder to the sidebar of Nemo, Nautilus and other GTK file managers.
settings-internet = Internet address
settings-internet-hint = DDNS name or public address with the forwarded SSH port, e.g. nas.example.org:2222. Used when the LAN address does not answer. QuickConnect does not carry SSH.
settings-internet-placeholder = nas.example.org:2222
settings-internet-invalid = This is not a valid address.
settings-cipher = Processor of the NAS
settings-cipher-x86 = Intel or AMD (AES-GCM first)
settings-cipher-arm = ARM (ChaCha20 first)
settings-cipher-unknown = Unknown (AES-GCM first)
settings-connections = Parallel connections
settings-connections-hint = Several SSH connections speed up many transfers at once. Needs sshfs 3.7 or newer.
settings-compress = Compress over the internet
settings-compress-hint = Helps on slow uplinks; costs processor time. Never used on the LAN.
settings-cache = Folder listings
settings-cache-normal = Cache for 20 seconds (faster)
settings-cache-fresh = Always fresh (several computers change the same folder)
settings-save = Save
settings-saved = Saved. A running mount connects again with the new settings.
settings-cancel = Cancel

## Remove

remove-title = Remove { $share }?
remove-text = The folder is unmounted and its service, bookmark and settings on this computer are removed. Files on the NAS stay as they are.
remove-key = Also remove this computer's key from the NAS
remove-key-hint = Only when no other folder of this NAS uses it. Afterwards this computer can no longer sign in without a password.
remove-confirm = Remove
remove-cancel = Cancel

## Command

command-title = The command behind { $share }
command-text = The service runs sshfs with these options (over the LAN address). Nothing else is changed on this computer.
command-copy = Copy
command-copied = Copied
command-close = Close

## Self-test

selftest-title = What { $share } can do
selftest-running = Testing in a temporary folder on the NAS …
selftest-intro = Tried for real in a temporary folder, which is removed again.
selftest-works = Works
selftest-limited = Limited
selftest-fails = Does not work
selftest-close = Close
selftest-again = Test again
check-write-read = Write and read files
check-write-read-fails = Files do not come back as they were written.
check-chmod = Unix permissions (chmod)
check-chmod-works = Permissions are kept as set.
check-chmod-fails = DSM ignores chmod in this folder because it uses Windows ACLs (DSM's default). Programs that rely on permissions may fail here. Folders without ACL keep Unix permissions, see the README.
check-owner = Own files show as yours
check-owner-limited = Files show a number instead of your user. Run the assistant again to map the owner.
check-mtime = Modification times
check-rename-over = Replace a file by renaming (how editors save)
check-symlink = Symbolic links
check-symlink-limited = Links are created, but DSM's SFTP server does not read them back. Programs that follow links on the NAS may fail.
check-hardlink = Hard links
check-xattr = Extended attributes (xattr)
check-xattr-fails = SFTP does not carry extended attributes. Programs that store metadata there (some games, macOS tags, SELinux labels) lose it.
check-lock = File locks
check-lock-limited = Locks work only on this computer; another computer does not see them. Do not open the same database or game save from two computers at once.
check-large-file = Files larger than 4 GB

## Speed test

speed-title = Speed of { $share }
speed-running = Writing and reading { $size } …
speed-write = Write
speed-read = Read
speed-small = Small files
speed-small-value = { $count } per second
speed-note = Reading may come partly from this computer's cache. Compare settings with the same size.
speed-size = Test size
speed-start = Start
speed-close = Close

## Add folders (assistant)

add-title = Add folders
add-step-nas = NAS
add-step-sign-in = Sign in
add-step-services = Services
add-step-key = Key
add-step-folders = Folders
add-step-done = Done
add-saved = Your NAS
add-found = On your network
add-searching = Searching the local network …
add-search-again = Search again
add-none-found = No NAS answered. A firewall on this computer can block the search (UDP port 9999 and Bonjour). You can always enter the address.
add-choose = Choose
add-forget = Remove from the list
add-manual = Enter an address
add-manual-placeholder = IP address or host name, e.g. 192.168.1.20 or nas.local:5001
add-manual-invalid = This is not a valid address.
add-cancel = Cancel
add-back = Back
add-next = Next

signin-title = Sign in to { $name }
signin-address = { $host }, port { $port }
signin-text = { -app-name } signs in to { -dsm } once to check the services and list your folders. The password is not stored.
signin-account = Account
signin-password = Password
signin-submit = Sign in
signin-busy = Signing in …
signin-otp-title = Two-step verification
signin-otp-text = Enter the code from your authenticator app.
signin-otp-code = Code
signin-otp-submit = Confirm
signin-cert-title = Unknown certificate
signin-cert-text = The NAS uses a certificate this computer does not know, usually DSM's own. Compare the fingerprint with Control Panel › Security › Certificate before you trust it.
signin-cert-changed-title = The certificate changed
signin-cert-changed-text = This NAS showed a different certificate before. If you did not renew it, someone may be in between. Only trust it if you know why it changed.
signin-cert-fingerprint = SHA-256 fingerprint
signin-cert-trust = Trust and continue
signin-cert-trust-changed = Trust the new certificate
signin-ssh-only = Without { -dsm } sign-in
signin-ssh-only-text = For accounts that may not use { -dsm }: { -app-name } goes by SFTP alone. Services cannot be checked then.
signin-not-allowed = { -dsm } does not let this account sign in. SFTP may still work; continue without { -dsm }.
signin-ssh-port = SSH port
signin-name = Name of the NAS
signin-continue = Continue

services-title = Services on { $name }
services-text = SSHFS needs SFTP and, for the key, a home folder for each user.
services-sftp = SFTP (Control Panel › File Services › FTP › SFTP)
services-home = User home (Control Panel › User & Group › Advanced)
services-on = On
services-off = Off
services-port = SFTP port { $port }
services-enable = Switch on what is off
services-enable-text = Changes in { -dsm }: { $changes }. Nothing else is changed.
services-unknown = Only administrators may read these settings. If the next step fails, ask an administrator to switch on SFTP and the user home service.
services-checking = Checking …

key-title = Trust { $name }
key-checking = Reading the host key …
key-text = Before { -app-name } sends a password, check that this is your NAS: compare the fingerprint, e.g. in a terminal on the NAS with ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub.
key-known = This key was trusted before.
key-changed = This NAS showed a different key before. Only continue if you know why it changed (for example a reinstalled DSM).
key-fingerprint = { $algorithm } fingerprint
key-password = SSH password
key-password-hint = Needed once to install the key. Leave empty to use the { -dsm } password.
key-install = Trust and install the key
key-installing = Installing the key …
key-explain = { -app-name } creates an ed25519 key for this computer, adds it to ~/.ssh/authorized_keys on the NAS and signs in with it. No password is stored.

folders-title = Choose folders
folders-text = Each folder is mounted in its own place on this computer.
folders-chrooted = { -dsm } shows this account its shared folders only, so the paths start at /.
folders-mounted = Mounted already
folders-local = Folder on this computer
folders-none = This account sees no shared folders over SFTP. Check its permissions in { -dsm } (Control Panel › Shared Folder and Application Privileges › SFTP).
folders-at-login = Mount at login and after suspend
folders-bookmark = Show in the file manager
folders-internet = Internet address (optional)
folders-create = Mount { $count ->
        [one] one folder
       *[other] { $count } folders
    }
folders-creating = Mounting …

done-title = Ready
done-text = { $count ->
        [one] One folder is
       *[other] { $count } folders are
    } set up. A self-test shows what each can do.
done-finish = Show the folders

## This computer

system-title = This computer
system-text = What SSHFS needs on this computer.
system-sshfs = sshfs
system-sshfs-version = Version { $version }
system-sshfs-old = Version { $version }: parallel connections need 3.7
system-missing = Missing
system-fusermount = fusermount3 (FUSE 3)
system-fuse-device = Access to /dev/fuse
system-systemd = systemd user services
system-systemd-missing = Not running: mounts cannot start at login.
system-ok = Present
system-install = Install what is missing:
system-again = Check again

## Settings

app-settings-title = Settings
app-settings-appearance = Appearance
settings-skin = Style
settings-skin-auto = Like the system ({ $skin })
settings-scheme = Colors
settings-scheme-auto = Like the system
settings-scheme-light = Light
settings-scheme-dark = Dark
settings-language = Language
settings-language-auto = Like the system
settings-version = Version { $version }
settings-license = Free software under the GNU GPL 3.0 or later.
settings-files = Files
settings-files-text = Keys, known_hosts and owner maps: ~/.local/share/syno-mount. Folder list: ~/.config/syno-mount. Services: ~/.config/systemd/user/syno-mount-*.service.

## Errors

error-network = The NAS cannot be reached. Check the address and whether it is switched on.
error-certificate-untrusted = The NAS presented an unknown certificate.
error-certificate-changed = The certificate of the NAS changed.
error-wrong-credentials = Account or password is wrong.
error-account-disabled = This account is disabled in { -dsm }.
error-account-not-allowed = { -dsm } does not let this account sign in.
error-otp-required = A two-step code is needed.
error-otp-wrong = The code is wrong or expired.
error-otp-enforced = { -dsm } requires two-step verification for this account; set it up in { -dsm } first.
error-ip-blocked = { -dsm } blocked this computer's address after failed sign-ins. Wait, or unblock it in Control Panel › Security.
error-password-expired = The password has expired; change it in { -dsm } first.
error-sign-in-failed = Signing in failed.
error-session-expired = The { -dsm } session ended. Start the assistant again.
error-permission-denied = Only administrators may do this.
error-unexpected = Something unexpected happened.
error-discovery = The network search could not start.
error-storage = The settings could not be saved.
error-not-connected = The assistant was interrupted. Start again.
error-not-found = This folder no longer exists.
error-ssh-network = The SSH service of the NAS does not answer. Is SFTP switched on, and is the port right?
error-host-key-unknown = The host key of the NAS has not been trusted yet.
error-host-key-changed = The host key of the NAS changed. { -app-name } does not connect until you trust the new key.
error-ssh-auth-failed = The NAS refused the sign-in. Check the password, and that SFTP is allowed for this account (Control Panel › Application Privileges).
error-sftp = The NAS refused a file operation.
error-io = A file on this computer could not be written.
error-selftest = The test could not run.
