// A stand-in for the Rust core in the browser preview, for working on the
// interface and for screenshots without a NAS. Only loaded by `npm run dev` with
// `?mock` in the address. Sign in with password "demo"; the NAS first shows an
// unknown certificate. `?mock=empty` starts without folders. For screenshots,
// `&shot=selftest` opens a self-test and `&shot=add` the assistant.

import type {
  Check,
  ConnectOutcome,
  ConnectRequest,
  CreateRequest,
  FoundNas,
  HostKeyView,
  Installation,
  KnownDevice,
  Mount,
  MountSettings,
  MountView,
  ServicesView,
  Speed,
  SystemCheck,
} from './backend';

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const FINGERPRINT = Array.from({ length: 32 }, (_, i) =>
  ((i * 37 + 11) % 256).toString(16).padStart(2, '0'),
)
  .join(':')
  .toUpperCase();

const found: FoundNas[] = [
  { name: 'nas-home', model: 'DS923+', host: '192.168.1.20', port: 5001, dsm: '7.3' },
  { name: 'nas-office', model: 'DS224+', host: '192.168.1.30', port: 5001, dsm: '7.3' },
];

function sample(nas: string, host: string, share: string, extra: Partial<Mount> = {}): Mount {
  const deviceId = `${host}:5001`;
  return {
    id: `${deviceId.replace(/[^a-z0-9]+/gi, '-')}-${share.toLowerCase()}`,
    deviceId,
    nasName: nas,
    share,
    remotePath: `/${share}`,
    localPath: `/home/anna/NAS/${nas}/${share}`,
    account: 'anna',
    lan: { host, port: 22 },
    hostKeyAlias: `syno-mount.${deviceId}`,
    hostKeyAlgorithm: 'ssh-ed25519',
    keyPath: `/home/anna/.local/share/syno-mount/keys/${nas}-anna`,
    remoteUid: 1026,
    remoteGid: 100,
    tuning: { cpu: 'x86', connections: 4, compressInternet: true, cache: 'normal' },
    atLogin: true,
    bookmark: true,
    ...extra,
  };
}

const empty = new URLSearchParams(location.search).get('mock') === 'empty';
let views: MountView[] = empty
  ? []
  : [
      {
        mount: sample('nas-home', '192.168.1.20', 'Media', {
          internet: { host: 'nas.example.org', port: 2222 },
        }),
        status: {
          id: '',
          state: 'mounted',
          connected: {
            route: 'lan',
            address: { host: '192.168.1.20', port: 22 },
            since: 1_793_000_000,
          },
          latencyMs: 0.6,
        },
      },
      {
        mount: sample('nas-home', '192.168.1.20', 'Games'),
        status: {
          id: '',
          state: 'mounted',
          connected: {
            route: 'lan',
            address: { host: '192.168.1.20', port: 22 },
            since: 1_793_000_000,
          },
          latencyMs: 0.7,
        },
      },
      {
        mount: sample('nas-office', '192.168.1.30', 'Projects', {
          internet: { host: 'office.example.org', port: 22 },
          tuning: { cpu: 'x86', connections: 4, compressInternet: true, cache: 'fresh' },
        }),
        status: {
          id: '',
          state: 'mounted',
          connected: {
            route: 'internet',
            address: { host: 'office.example.org', port: 22 },
            since: 1_793_000_000,
          },
          latencyMs: 23,
        },
      },
    ];
for (const view of views) view.status.id = view.mount.id;

let saved: KnownDevice[] = [];
let trusted = false;
let pending: { device: KnownDevice; sftp: boolean; home: boolean } | null = null;

async function connect(request: ConnectRequest): Promise<ConnectOutcome> {
  await wait(500);
  if (!trusted && request.trustFingerprint !== FINGERPRINT) {
    return { status: 'untrustedCertificate', fingerprint: FINGERPRINT };
  }
  trusted = true;
  if (request.password !== 'demo') return { status: 'failed', key: 'wrong-credentials' };
  const device: KnownDevice = {
    id: `${request.host.toLowerCase()}:${request.port}`,
    name: request.name ?? request.host,
    model: request.model ?? '',
    host: request.host,
    port: request.port,
    account: request.account,
    fingerprint: FINGERPRINT,
  };
  saved = [device, ...saved.filter((known) => known.id !== device.id)];
  pending = { device, sftp: true, home: false };
  return { status: 'connected', device };
}

function services(): ServicesView {
  return {
    services: { sftp: pending?.sftp ?? false, sftpPort: 22, home: pending?.home ?? false },
    sshPort: 22,
  };
}

const hostKey: HostKeyView = {
  key: {
    openssh: 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample',
    algorithm: 'ssh-ed25519',
    fingerprint: 'SHA256:ZY6qLmeMnD3Vc04IE19Ps52npfm+6LfRSr61JVqpASc',
  },
  known: false,
  changed: false,
};

function installation(): Installation {
  const nas = pending?.device.name ?? 'nas-home';
  return {
    chrooted: true,
    shares: ['Games', 'home', 'Media', 'Photos', 'Projects'].map((name) => ({
      name,
      remotePath: `/${name}`,
      defaultFolder: `/home/anna/NAS/${nas}/${name}`,
      mounted: views.some((view) => view.mount.share === name && view.mount.nasName === nas),
    })),
  };
}

function create(request: CreateRequest): Mount[] {
  const device = pending?.device;
  const created = request.shares.map((share) =>
    sample(device?.name ?? 'nas-home', device?.host ?? '192.168.1.20', share.name, {
      localPath: share.localPath,
      atLogin: request.atLogin,
      bookmark: request.bookmark,
      internet: request.internet,
    }),
  );
  for (const mount of created) {
    views.push({
      mount,
      status: {
        id: mount.id,
        state: 'mounted',
        connected: { route: 'lan', address: mount.lan, since: 1_793_000_000 },
        latencyMs: 0.8,
      },
    });
  }
  pending = null;
  return created;
}

const checks: Check[] = [
  { id: 'write-read', outcome: 'works' },
  { id: 'chmod', outcome: 'works' },
  { id: 'owner', outcome: 'works' },
  { id: 'mtime', outcome: 'works' },
  { id: 'rename-over', outcome: 'works' },
  { id: 'symlink', outcome: 'limited', detail: 'read back: No such file or directory' },
  { id: 'hardlink', outcome: 'works' },
  { id: 'xattr', outcome: 'fails', detail: 'Operation not supported (os error 95)' },
  { id: 'lock', outcome: 'limited' },
  { id: 'large-file', outcome: 'works' },
];

const speed: Speed = {
  writeBytesPerSecond: 112_400_000,
  readBytesPerSecond: 117_800_000,
  smallFilesPerSecond: 386,
  megabytes: 128,
};

const system: SystemCheck = {
  sshfs: '3.7.3',
  sshfsParallel: true,
  fusermount: true,
  fuseDevice: true,
  systemdUser: true,
  installHint: null,
};

function find(id: unknown): MountView {
  const view = views.find((entry) => entry.mount.id === id);
  if (!view) throw { key: 'not-found' };
  return view;
}

const commands: Record<string, (args: Record<string, unknown>) => Promise<unknown>> = {
  discover: async () => (await wait(1000), found),
  known_devices: async () => saved,
  forget_device: async (args) => {
    saved = saved.filter((device) => device.id !== args.id);
  },
  connect: (args) => connect(args.request as ConnectRequest),
  connect_ssh_only: async () => {
    throw { key: 'ssh-auth-failed' };
  },
  setup_services: async () => (await wait(300), services()),
  enable_services: async () => {
    await wait(700);
    if (pending) pending = { ...pending, sftp: true, home: true };
    return services();
  },
  scan_host_key: async () => (await wait(400), hostKey),
  install_key: async () => (await wait(900), installation()),
  more_shares: async (args) => {
    const view = find(args.mountId);
    pending = {
      device: {
        id: view.mount.deviceId,
        name: view.mount.nasName,
        model: '',
        host: view.mount.lan.host,
        port: 5001,
        account: view.mount.account,
      },
      sftp: true,
      home: true,
    };
    return installation();
  },
  create_mounts: async (args) => (await wait(800), create(args.request as CreateRequest)),
  cancel_setup: async () => {
    pending = null;
  },
  system_check: async () => system,
  list_mounts: async () => structuredClone(views),
  start_mount: async (args) => {
    const view = find(args.id);
    view.status = {
      ...view.status,
      state: 'mounted',
      connected: { route: 'lan', address: view.mount.lan, since: 1_793_000_000 },
      latencyMs: 0.7,
    };
  },
  stop_mount: async (args) => {
    const view = find(args.id);
    view.status = { id: view.mount.id, state: 'stopped' };
  },
  update_mount: async (args) => {
    const view = find(args.id);
    const settings = args.settings as MountSettings;
    view.mount = { ...view.mount, ...settings };
    return view.mount;
  },
  remove_mount: async (args) => {
    views = views.filter((view) => view.mount.id !== args.id);
  },
  self_test: async () => (await wait(1200), checks),
  speed_test: async () => (await wait(1500), speed),
  open_folder: async () => undefined,
  command_line: async (args) => {
    const mount = find(args.id).mount;
    return `sshfs -f -o Port=22 -o IdentityFile=${mount.keyPath} -o IdentitiesOnly=yes -o UserKnownHostsFile=/home/anna/.local/share/syno-mount/known_hosts -o HostKeyAlias=${mount.hostKeyAlias} -o StrictHostKeyChecking=yes -o BatchMode=yes -o reconnect -o ServerAliveInterval=15 -o max_conns=4 -o idmap=file ${mount.account}@${mount.lan.host}:${mount.remotePath} ${mount.localPath}`;
  },
};

(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
  invoke: (command: string, args: Record<string, unknown> = {}) => {
    const handler = commands[command];
    return handler ? handler(args) : Promise.reject({ key: 'unexpected', detail: command });
  },
};

// Screenshots: open a view by clicking through it once the page is up.
const shot = new URLSearchParams(location.search).get('shot');
if (shot) {
  const click = (selector: string, index = 0) =>
    (document.querySelectorAll<HTMLElement>(selector)[index] ?? null)?.click();
  setTimeout(() => {
    if (shot === 'selftest') {
      const buttons = [...document.querySelectorAll<HTMLButtonElement>('.mount .actions > button')];
      buttons.filter((button) => !button.disabled)[5]?.click();
    }
    if (shot === 'add') click('.nav-item', 1);
  }, 600);
}
