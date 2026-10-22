import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Progress =
  | { stage: "checking"; done: number; total: number }
  | { stage: "downloading"; source: string; done: number; total: number }
  | { stage: "unpacking"; source: string }
  | { stage: "building"; done: number; total: number }
  | { stage: "cleaningUp" };

export type Status =
  | { kind: "starting" }
  | { kind: "updatingLauncher" }
  | { kind: "needsFolder"; suggested: string }
  | { kind: "working"; progress: Progress }
  | { kind: "ready"; offline: boolean; note: string | null }
  | { kind: "error"; message: string };

export type Resolution = { kind: "auto" } | { kind: "native" } | { kind: "fixed"; width: number; height: number };

export interface Settings {
  gameFolder: string | null;
  resolution: Resolution;
  fullscreen: boolean;
  serverOverride: string | null;
}

export interface Info {
  message: string;
  game: string;
  launcherVersion: string;
}

export interface ScreenMode {
  width: number;
  height: number;
}

export interface DisplayOptions {
  native: ScreenMode;
  auto: ScreenMode;
  modes: ScreenMode[];
}

export interface FolderCheck {
  exists: boolean;
  empty: boolean;
  hasGame: boolean;
  freeBytes: number | null;
  neededBytes: number;
}

export const api = {
  status: () => invoke<Status>("get_status"),
  info: () => invoke<Info | null>("get_info"),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  retry: () => invoke<void>("retry"),
  repair: () => invoke<void>("repair"),
  cancel: () => invoke<void>("cancel"),
  pickFolder: () => invoke<string | null>("pick_folder"),
  pickInstaller: () => invoke<string | null>("pick_installer"),
  checkFolder: (path: string) => invoke<FolderCheck>("check_folder", { path }),
  install: (folder: string, copy: string | null) => invoke<void>("install", { folder, copy }),
  displayOptions: () => invoke<DisplayOptions>("display_options"),
  serverStatus: () => invoke<boolean | null>("server_status"),
  play: () => invoke<void>("play"),
  openLogs: () => invoke<void>("open_logs"),
  onStatus: (cb: (s: Status) => void): Promise<UnlistenFn> => listen<Status>("status", (e) => cb(e.payload)),
};
