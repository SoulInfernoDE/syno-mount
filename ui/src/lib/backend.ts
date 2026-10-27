// Calls into the Rust core. In a plain browser (`npm run dev`) there is no core;
// open `/?mock` for a stand-in with sample data.

import { invoke } from '@tauri-apps/api/core';

/** True inside the app (or with the mock); false in the plain browser preview. */
export const hasCore = () => '__TAURI_INTERNALS__' in window;

/** An error from the core: `key` selects the message `error-<key>`. */
export interface Failure {
  key: string;
  detail?: string;
}

export function isFailure(value: unknown): value is Failure {
  return typeof value === 'object' && value !== null && 'key' in value;
}

/** Turns anything thrown by `invoke` into a `Failure`. */
export function toFailure(error: unknown): Failure {
  return isFailure(error) ? error : { key: 'unexpected', detail: String(error) };
}

export interface FoundNas {
  name: string;
  model: string;
  host: string;
  port: number;
  dsm: string | null;
}

export interface KnownDevice {
  id: string;
  name: string;
  model: string;
  host: string;
  port: number;
  account: string;
  fingerprint?: string;
}

export interface Address {
  host: string;
  port: number;
}

export type CpuKind = 'x86' | 'arm' | 'unknown';
export type Cache = 'normal' | 'fresh';

export interface Tuning {
  cpu: CpuKind;
  connections: number;
  compressInternet: boolean;
  cache: Cache;
}

export interface Mount {
  id: string;
  deviceId: string;
  nasName: string;
  share: string;
  remotePath: string;
  localPath: string;
  account: string;
  lan: Address;
  internet?: Address;
  hostKeyAlias: string;
  hostKeyAlgorithm: string;
  keyPath: string;
  remoteUid?: number;
  remoteGid?: number;
  tuning: Tuning;
  atLogin: boolean;
  bookmark: boolean;
}

export type MountState = 'mounted' | 'stale' | 'connecting' | 'stopped' | 'failed';
export type Route = 'lan' | 'internet';

export interface MountStatus {
  id: string;
  state: MountState;
  connected?: { route: Route; address: Address; since: number };
  latencyMs?: number;
  log?: string;
}

export interface MountView {
  mount: Mount;
  status: MountStatus;
}

export interface SystemCheck {
  sshfs: string | null;
  sshfsParallel: boolean;
  fusermount: boolean;
  fuseDevice: boolean;
  systemdUser: boolean;
  installHint: string | null;
}

export type Outcome = 'works' | 'limited' | 'fails';

export interface Check {
  id: string;
  outcome: Outcome;
  detail?: string;
}

export interface Speed {
  writeBytesPerSecond: number;
  readBytesPerSecond: number;
  smallFilesPerSecond: number;
  megabytes: number;
}

export interface ConnectRequest {
  host: string;
  port: number;
  name?: string;
  model?: string;
  account: string;
  password: string;
  otp?: string;
  trustFingerprint?: string;
}

export type ConnectOutcome =
  | { status: 'connected'; device: KnownDevice }
  | { status: 'otpRequired' }
  | { status: 'untrustedCertificate'; fingerprint: string }
  | { status: 'certificateChanged'; fingerprint: string }
  | { status: 'notAllowed' }
  | ({ status: 'failed' } & Failure);

export interface SshOnlyRequest {
  host: string;
  dsmPort: number;
  sshPort: number;
  name?: string;
  account: string;
  password: string;
}

export interface Services {
  sftp: boolean;
  sftpPort: number;
  home: boolean;
}

export interface ServicesView {
  services: Services | null;
  sshPort: number;
}

export interface HostKey {
  openssh: string;
  algorithm: string;
  fingerprint: string;
}

export interface HostKeyView {
  key: HostKey;
  known: boolean;
  changed: boolean;
}

export interface ShareChoice {
  name: string;
  remotePath: string;
  defaultFolder: string;
  mounted: boolean;
}

export interface Installation {
  shares: ShareChoice[];
  chrooted: boolean;
}

export interface CreateRequest {
  shares: { name: string; localPath: string }[];
  atLogin: boolean;
  bookmark: boolean;
  internet?: Address;
}

export interface MountSettings {
  atLogin: boolean;
  bookmark: boolean;
  internet?: Address;
  tuning: Tuning;
}

export const discover = () => invoke<FoundNas[]>('discover');
export const knownDevices = () => invoke<KnownDevice[]>('known_devices');
export const forgetDevice = (id: string) => invoke<void>('forget_device', { id });
export const connect = (request: ConnectRequest) => invoke<ConnectOutcome>('connect', { request });
export const connectSshOnly = (request: SshOnlyRequest) =>
  invoke<KnownDevice>('connect_ssh_only', { request });
export const setupServices = () => invoke<ServicesView>('setup_services');
export const enableServices = () => invoke<ServicesView>('enable_services');
export const scanHostKey = (port?: number) => invoke<HostKeyView>('scan_host_key', { port });
export const installKey = (trustFingerprint: string, password?: string) =>
  invoke<Installation>('install_key', { request: { trustFingerprint, password } });
export const moreShares = (mountId: string) => invoke<Installation>('more_shares', { mountId });
export const createMounts = (request: CreateRequest) =>
  invoke<Mount[]>('create_mounts', { request });
export const cancelSetup = () => invoke<void>('cancel_setup');

export const systemCheck = () => invoke<SystemCheck>('system_check');
export const listMounts = () => invoke<MountView[]>('list_mounts');
export const startMount = (id: string) => invoke<void>('start_mount', { id });
export const stopMount = (id: string) => invoke<void>('stop_mount', { id });
export const updateMount = (id: string, settings: MountSettings) =>
  invoke<Mount>('update_mount', { id, settings });
export const removeMount = (id: string, removeKey: boolean) =>
  invoke<void>('remove_mount', { id, removeKey });
export const selfTest = (id: string) => invoke<Check[]>('self_test', { id });
export const speedTest = (id: string, megabytes?: number) =>
  invoke<Speed>('speed_test', { id, megabytes });
export const openFolder = (id: string) => invoke<void>('open_folder', { id });
export const commandLine = (id: string) => invoke<string>('command_line', { id });

/** "nas.local:5001", "192.168.1.20" or "[fe80::1]:5001" → host and port. */
export function parseAddress(
  input: string,
  defaultPort = 5001,
): { host: string; port: number } | null {
  let text = input
    .trim()
    .replace(/^(https?|ssh|sftp):\/\//i, '')
    .replace(/\/.*$/, '');
  if (!text) return null;
  let port = defaultPort;
  const bracketed = /^\[([^\]]+)\](?::(\d+))?$/.exec(text);
  if (bracketed) {
    text = bracketed[1]!;
    if (bracketed[2]) port = Number(bracketed[2]);
  } else if ((text.match(/:/g) ?? []).length === 1) {
    const [host, portText] = text.split(':');
    text = host!;
    port = Number(portText);
  }
  if (!text || !Number.isInteger(port) || port < 1 || port > 65535) return null;
  return { host: text, port };
}
