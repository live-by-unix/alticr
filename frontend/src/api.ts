import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface SessionInfo {
  id: string;
  window_label: string;
  shell: string;
  cwd: string;
  cols: number;
  rows: number;
  pid: number;
  created_at: number;
  state: "starting" | "running" | "exited" | "failed";
  exit_code: number | null;
  title: string;
  fg_process: string | null;
  fg_pid: number | null;
}

export interface StatusEvent {
  session_id: string;
  state: SessionInfo["state"];
  exit_code: number | null;
  title: string;
  cwd: string;
  fg_process: string | null;
  fg_pid: number | null;
}

export interface OutputEvent {
  session_id: string;
  data: string; // base64
}

export interface ClipboardEvent {
  session_id: string;
  text: string; // raw OSC 52 payload "c;<data>"
}

export interface ShellInfo {
  name: string;
  path: string;
  is_default: boolean;
}

export interface AppSettings {
  theme: string;
  accent_color: string;
  font_family: string;
  font_size: number;
  line_height: number;
  cursor_style: string;
  cursor_blink: boolean;
  window_opacity: number;
  padding: number;
  default_shell: string;
  shell_args: string;
  startup_dir: string;
  scrollback_lines: number;
  env_vars: Record<string, string>;
  bell: string;
  tab_show_process: boolean;
  tab_show_shell: boolean;
  confirm_close_running: boolean;
  shortcuts: Record<string, string>;
  allow_osc52_clipboard: boolean;
  debug_logging: boolean;
}

export const api = {
  createSession: (args: {
    window_label: string;
    shell?: string;
    args?: string;
    cwd?: string;
    cols: number;
    rows: number;
    env?: [string, string][];
  }) => invoke<SessionInfo>("create_session", { args }),

  writeSession: (session_id: string, data_b64: string) =>
    invoke<void>("write_session", { sessionId: session_id, dataB64: data_b64 }),

  resizeSession: (session_id: string, cols: number, rows: number) =>
    invoke<void>("resize_session", { sessionId: session_id, cols, rows }),

  closeSession: (session_id: string) =>
    invoke<void>("close_session", { sessionId: session_id }),

  listSessions: (window_label: string) =>
    invoke<SessionInfo[]>("list_sessions", { windowLabel: window_label }),

  exportSession: (session_id: string) =>
    invoke<string>("export_session", { sessionId: session_id }),

  getSettings: () => invoke<AppSettings>("get_settings"),
  setSettings: (settings: Record<string, unknown>) =>
    invoke<AppSettings>("set_settings", { settings }),

  detectShells: () => invoke<ShellInfo[]>("detect_shells"),

  closeWindowSessions: (window_label: string) =>
    invoke<void>("close_window_sessions", { windowLabel: window_label }),

  focusWindow: (label: string) => invoke<void>("focus_window", { label }),
};

export function onOutput(cb: (e: OutputEvent) => void): Promise<UnlistenFn> {
  return listen<OutputEvent>("alticr://output", (e) => cb(e.payload));
}
export function onStatus(cb: (e: StatusEvent) => void): Promise<UnlistenFn> {
  return listen<StatusEvent>("alticr://status", (e) => cb(e.payload));
}
export function onClipboard(cb: (e: ClipboardEvent) => void): Promise<UnlistenFn> {
  return listen<ClipboardEvent>("alticr://clipboard", (e) => cb(e.payload));
}
export function onMenuAction(cb: (action: string) => void): Promise<UnlistenFn> {
  return listen<string>("alticr://menu-action", (e) => cb(e.payload));
}

// ---- base64 helpers ------------------------------------------------------

export function bytesToB64(bytes: Uint8Array): string {
  let bin = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(bin);
}

export function b64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function strToB64(s: string): string {
  return bytesToB64(new TextEncoder().encode(s));
}
