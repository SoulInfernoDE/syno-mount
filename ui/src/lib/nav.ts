/** The sections of the main window. */
export type Section = 'mounts' | 'add' | 'admin' | 'system' | 'settings';

/** What the menu of a mount card asks for. */
export type MountAction = 'selftest' | 'speed' | 'settings' | 'command' | 'remove' | 'more';

/** A NAS to sign in to: found on the network, saved, or typed in. */
export interface NasTarget {
  host: string;
  port: number;
  name?: string;
  model?: string;
  account?: string;
}
