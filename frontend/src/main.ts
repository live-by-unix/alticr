import "@xterm/xterm/css/xterm.css";
import "./styles.css";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  api,
  b64ToBytes,
  onClipboard,
  onOutput,
  onStatus,
  strToB64,
  type AppSettings,
  type SessionInfo,
} from "./api";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type SessionState = SessionInfo["state"];

interface Tab {
  sessionId: string;
  shell: string;
  title: string;
  state: SessionState;
  exitCode: number | null;
  cwd: string;
  fgProcess: string | null;
  term: Terminal;
  fit: FitAddon;
  search: SearchAddon;
  host: HTMLElement;
  cols: number;
  rows: number;
  createdAt: number;
}

const win = getCurrentWindow();
const WINDOW_LABEL = win.label;

let settings: AppSettings;
let tabs: Tab[] = [];
let activeIdx = -1;

const $ = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T;

const termsEl = $<HTMLElement>("terms");
const tabsEl = $<HTMLElement>("tabs");

// ---------------------------------------------------------------------------
// Settings application
// ---------------------------------------------------------------------------

function applySettings(s: AppSettings) {
  settings = s;
  document.body.classList.toggle("light", s.theme === "light");
  document.documentElement.style.setProperty("--accent", s.accent_color);
  document.documentElement.style.setProperty("--pad", `${s.padding}px`);

  for (const t of tabs) {
    t.term.options.fontFamily =
      s.font_family || "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";
    t.term.options.fontSize = s.font_size;
    t.term.options.lineHeight = s.line_height;
    t.term.options.cursorStyle = s.cursor_style as "block" | "underline" | "bar";
    t.term.options.cursorBlink = s.cursor_blink;
    scheduleFit(t);
  }
}

// ---------------------------------------------------------------------------
// Terminal creation
// ---------------------------------------------------------------------------

function xtermTheme(): { [key: string]: string } {
  if (settings.theme === "light") {
    return {
      background: "#ffffff",
      foreground: "#1c2530",
      cursor: "#1c2530",
      cursorAccent: "#ffffff",
      selectionBackground: "rgba(56,189,248,0.35)",
      black: "#1c2530", red: "#dc2626", green: "#16a34a", yellow: "#ca8a04",
      blue: "#2563eb", magenta: "#9333ea", cyan: "#0891b2", white: "#e5e9f0",
      brightBlack: "#64748b", brightRed: "#ef4444", brightGreen: "#22c55e",
      brightYellow: "#eab308", brightBlue: "#3b82f6", brightMagenta: "#a855f7",
      brightCyan: "#06b6d4", brightWhite: "#f8fafc",
    };
  }
  return {
    background: "#0b0e12",
    foreground: "#d7dee6",
    cursor: "#38bdf8",
    cursorAccent: "#0b0e12",
    selectionBackground: "rgba(56,189,248,0.3)",
    black: "#1c2530", red: "#f87171", green: "#34d399", yellow: "#fbbf24",
    blue: "#60a5fa", magenta: "#c084fc", cyan: "#22d3ee", white: "#e2e8f0",
    brightBlack: "#64748b", brightRed: "#fca5a5", brightGreen: "#6ee7b7",
    brightYellow: "#fde047", brightBlue: "#93c5fd", brightMagenta: "#d8b4fe",
    brightCyan: "#67e8f9", brightWhite: "#f8fafc",
  };
}

function makeTerm(): { term: Terminal; fit: FitAddon; search: SearchAddon } {
  const term = new Terminal({
    fontFamily:
      settings.font_family ||
      "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
    fontSize: settings.font_size,
    lineHeight: settings.line_height,
    cursorStyle: settings.cursor_style as "block" | "underline" | "bar",
    cursorBlink: settings.cursor_blink,
    scrollback: settings.scrollback_lines,
    theme: xtermTheme(),
    allowProposedApi: true,
    macOptionIsMeta: true,
  });
  const fit = new FitAddon();
  const search = new SearchAddon();
  term.loadAddon(fit);
  term.loadAddon(search);
  term.loadAddon(new WebLinksAddon());
  return { term, fit, search };
}

async function createSession(opts?: {
  shell?: string;
  args?: string;
  cwd?: string;
}): Promise<Tab | null> {
  // Measure the terminal area to request a reasonable initial PTY size.
  const rect = termsEl.getBoundingClientRect();
  const pad = settings.padding * 2;
  const approxCols = Math.max(20, Math.floor((rect.width - pad) / 8));
  const approxRows = Math.max(5, Math.floor((rect.height - pad) / 17));

  let info: SessionInfo;
  try {
    info = await api.createSession({
      window_label: WINDOW_LABEL,
      shell: opts?.shell,
      args: opts?.args,
      cwd: opts?.cwd,
      cols: approxCols,
      rows: approxRows,
    });
  } catch (e) {
    console.error("create_session failed:", e);
    showToast(`Failed to start shell: ${e}`);
    return null;
  }

  const { term, fit, search } = makeTerm();
  const host = document.createElement("div");
  host.className = "term-host";
  termsEl.appendChild(host);
  term.open(host);

  const tab: Tab = {
    sessionId: info.id,
    shell: info.shell,
    title: info.title,
    state: info.state,
    exitCode: info.exit_code,
    cwd: info.cwd,
    fgProcess: info.fg_process,
    term,
    fit,
    search,
    host,
    cols: info.cols,
    rows: info.rows,
    createdAt: info.created_at,
  };

  term.onData((data) => {
    api.writeSession(tab.sessionId, strToB64(data)).catch(() => {});
  });
  term.onBell(() => flashTab(tab));
  term.onTitleChange((t) => {
    tab.title = t;
    renderTabs();
  });

  tabs.push(tab);
  activateTab(tabs.length - 1);

  // Fit now that the element has real dimensions, then resize the PTY.
  requestAnimationFrame(() => {
    fitTab(tab);
    api
      .resizeSession(tab.sessionId, term.cols, term.rows)
      .catch(() => {});
  });

  renderTabs();
  renderStatus();
  return tab;
}

async function closeTab(idx: number) {
  const tab = tabs[idx];
  if (!tab) return;
  if (
    settings.confirm_close_running &&
    (tab.state === "running" || tab.state === "starting")
  ) {
    const name = tabLabel(tab);
    if (!window.confirm(`Close terminal "${name}"? The running process will be terminated.`)) {
      return;
    }
  }
  tabs.splice(idx, 1);
  tab.host.remove();
  tab.term.dispose();
  api.closeSession(tab.sessionId).catch(() => {});

  if (activeIdx >= tabs.length) activeIdx = tabs.length - 1;
  if (activeIdx >= 0) activateTab(activeIdx);
  else activeIdx = -1;
  renderTabs();
  renderStatus();
}

function activateTab(idx: number) {
  if (idx < 0 || idx >= tabs.length) return;
  activeIdx = idx;
  tabs.forEach((t, i) => {
    t.host.classList.toggle("active", i === idx);
  });
  const tab = tabs[idx];
  tab.term.focus();
  fitTab(tab);
  api.resizeSession(tab.sessionId, tab.term.cols, tab.term.rows).catch(() => {});
  renderTabs();
  renderStatus();
  updateWindowTitle();
}

function fitTab(tab: Tab) {
  try {
    tab.fit.fit();
  } catch {
    /* element may be hidden */
  }
}

let fitQueued = new Set<Tab>();
function scheduleFit(tab: Tab) {
  fitQueued.add(tab);
  if (fitQueued.size === 1) {
    requestAnimationFrame(() => {
      const batch = [...fitQueued];
      fitQueued.clear();
      for (const t of batch) {
        const old = { c: t.term.cols, r: t.term.rows };
        fitTab(t);
        if (t.term.cols !== old.c || t.term.rows !== old.r) {
          api.resizeSession(t.sessionId, t.term.cols, t.term.rows).catch(() => {});
        }
      }
    });
  }
}

// ---------------------------------------------------------------------------
// Rendering: tabs & status bar
// ---------------------------------------------------------------------------

function shellBase(shell: string): string {
  const parts = shell.split(/[\\/]/);
  return parts[parts.length - 1] || shell;
}

function tabLabel(tab: Tab): string {
  const parts: string[] = [];
  if (settings.tab_show_process) {
    parts.push(tab.fgProcess || shellBase(tab.shell));
  } else {
    parts.push(shellBase(tab.shell));
  }
  if (settings.tab_show_shell && tab.fgProcess) {
    parts.push(shellBase(tab.shell));
  }
  if (tab.title && !parts.includes(tab.title)) {
    // Title often carries useful info (e.g. "vim file.rs"); prefer fg proc.
  }
  return parts.join(" · ");
}

function renderTabs() {
  tabsEl.innerHTML = "";
  tabs.forEach((tab, i) => {
    const el = document.createElement("button");
    el.className = `tab ${tab.state}${i === activeIdx ? " active" : ""}`;
    el.draggable = true;
    el.title = `${tab.shell} — ${tab.cwd}${tab.title ? ` — ${tab.title}` : ""}`;

    const dot = document.createElement("span");
    dot.className = "tab-dot";
    const label = document.createElement("span");
    label.className = "tab-label";
    label.textContent = tabLabel(tab);
    const close = document.createElement("span");
    close.className = "tab-close";
    close.textContent = "×";
    close.title = "Close tab";

    el.append(dot, label, close);
    el.addEventListener("click", (e) => {
      if ((e.target as HTMLElement).classList.contains("tab-close")) return;
      activateTab(i);
    });
    close.addEventListener("click", (e) => {
      e.stopPropagation();
      closeTab(i);
    });
    el.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      showTabMenu(e.clientX, e.clientY, i);
    });
    // Drag reorder
    el.addEventListener("dragstart", (e) => {
      e.dataTransfer?.setData("text/plain", String(i));
      el.classList.add("dragging");
    });
    el.addEventListener("dragend", () => el.classList.remove("dragging"));
    el.addEventListener("dragover", (e) => {
      e.preventDefault();
      el.classList.add("drag-over");
    });
    el.addEventListener("dragleave", () => el.classList.remove("drag-over"));
    el.addEventListener("drop", (e) => {
      e.preventDefault();
      el.classList.remove("drag-over");
      const from = Number(e.dataTransfer?.getData("text/plain"));
      if (Number.isNaN(from) || from === i) return;
      const [moved] = tabs.splice(from, 1);
      tabs.splice(i, 0, moved);
      if (activeIdx === from) activeIdx = i;
      else if (from < activeIdx && i >= activeIdx) activeIdx--;
      else if (from > activeIdx && i <= activeIdx) activeIdx++;
      renderTabs();
    });
    tabsEl.appendChild(el);
  });
}

function renderStatus() {
  const tab = tabs[activeIdx];
  if (!tab) {
    $("st-shell").textContent = "";
    $("st-cwd").textContent = "";
    $("st-process").textContent = "";
    $("st-dims").textContent = "";
    $("st-state").textContent = "";
    return;
  }
  $("st-shell").textContent = shellBase(tab.shell);
  $("st-cwd").textContent = tab.cwd || "";
  $("st-process").textContent = tab.fgProcess
    ? `⚙ ${tab.fgProcess}`
    : "";
  $("st-dims").textContent = `${tab.term.cols} × ${tab.term.rows}`;
  const stateTxt =
    tab.state === "running"
      ? "● running"
      : tab.state === "exited"
        ? `○ exited${tab.exitCode != null ? ` (${tab.exitCode})` : ""}`
        : tab.state === "failed"
          ? "✕ failed"
          : "… starting";
  $("st-state").textContent = stateTxt;
  $("st-state").style.color =
    tab.state === "exited" || tab.state === "failed" ? "var(--danger)" : "";
}

function flashTab(_tab: Tab) {
  const el = tabsEl.children[activeIdx] as HTMLElement | undefined;
  if (!el) return;
  el.style.transition = "background 0.15s";
  el.style.background = "var(--accent-dim)";
  setTimeout(() => (el.style.background = ""), 180);
}

function updateWindowTitle() {
  const tab = tabs[activeIdx];
  const name = tab ? tabLabel(tab) : "ALTICR";
  document.title = `${name} — ALTICR`;
  win.setTitle(`${name} — ALTICR`).catch(() => {});
}

// ---------------------------------------------------------------------------
// Backend events
// ---------------------------------------------------------------------------

function findTab(sessionId: string): Tab | undefined {
  return tabs.find((t) => t.sessionId === sessionId);
}

async function wireEvents() {
  await onOutput((e) => {
    const tab = findTab(e.session_id);
    if (!tab) return;
    tab.term.write(b64ToBytes(e.data));
  });

  await onStatus((e) => {
    const tab = findTab(e.session_id);
    if (!tab) return;
    const changed =
      tab.state !== e.state ||
      tab.title !== e.title ||
      tab.cwd !== e.cwd ||
      tab.fgProcess !== e.fg_process ||
      tab.exitCode !== e.exit_code;
    tab.state = e.state;
    tab.title = e.title;
    tab.cwd = e.cwd;
    tab.fgProcess = e.fg_process;
    tab.exitCode = e.exit_code;
    if (changed) {
      renderTabs();
      renderStatus();
      updateWindowTitle();
    }
  });

  await onClipboard((e) => {
    // OSC 52: payload looks like "c;<data>" where <data> may be
    // base64 (prefixed with '?') or plain text.
    if (!settings.allow_osc52_clipboard) return;
    const semi = e.text.indexOf(";");
    if (semi < 0) return;
    let payload = e.text.slice(semi + 1);
    try {
      if (payload.startsWith("?")) {
        payload = new TextDecoder().decode(
          Uint8Array.from(atob(payload.slice(1)), (c) => c.charCodeAt(0)),
        );
      }
      navigator.clipboard.writeText(payload).catch(() => {});
    } catch {
      /* malformed payload */
    }
  });
}

// ---------------------------------------------------------------------------
// Context menus
// ---------------------------------------------------------------------------

const ctxMenu = $("ctx-menu");

function showMenu(x: number, y: number, items: { label: string; danger?: boolean; action: () => void }[]) {
  ctxMenu.innerHTML = "";
  items.forEach((it, i) => {
    if (it.label === "-") {
      const sep = document.createElement("div");
      sep.className = "ctx-sep";
      ctxMenu.appendChild(sep);
      return;
    }
    const b = document.createElement("button");
    b.className = "ctx-item" + (it.danger ? " danger" : "");
    b.textContent = it.label;
    b.addEventListener("click", () => {
      hideMenu();
      it.action();
    });
    ctxMenu.appendChild(b);
    void i;
  });
  ctxMenu.classList.remove("hidden");
  const r = ctxMenu.getBoundingClientRect();
  ctxMenu.style.left = `${Math.min(x, innerWidth - r.width - 8)}px`;
  ctxMenu.style.top = `${Math.min(y, innerHeight - r.height - 8)}px`;
}

function hideMenu() {
  ctxMenu.classList.add("hidden");
}
document.addEventListener("click", (e) => {
  if (!ctxMenu.contains(e.target as Node)) hideMenu();
});
document.addEventListener("contextmenu", (e) => {
  if (!(e.target as HTMLElement).closest(".term-host") &&
      !(e.target as HTMLElement).closest(".tab")) {
    hideMenu();
  }
});

function showTabMenu(x: number, y: number, idx: number) {
  showMenu(x, y, [
    { label: "New tab", action: () => quickNewTab() },
    { label: "-", action: () => {} },
    { label: "Close", action: () => closeTab(idx) },
    {
      label: "Close others",
      action: () => {
        const keep = tabs[idx];
        [...tabs.keys()].reverse().forEach((i) => {
          if (i !== idx) void closeTab(i);
        });
        void keep;
      },
    },
    { label: "-", action: () => {} },
    { label: "New window", action: () => void newWindow() },
  ]);
}

function termMenu(tab: Tab) {
  return [
    {
      label: "Copy",
      action: () => {
        const sel = tab.term.getSelection();
        if (sel) navigator.clipboard.writeText(sel).catch(() => {});
      },
    },
    {
      label: "Paste",
      action: () => {
        navigator.clipboard
          .readText()
          .then((t) => tab.term.paste(t))
          .catch(() => {});
      },
    },
    { label: "Select all", action: () => tab.term.selectAll() },
    { label: "-", action: () => {} },
    { label: "Clear screen", action: () => tab.term.clear() },
    {
      label: "Reset terminal",
      action: () => api.writeSession(tab.sessionId, strToB64("\x1bc")).catch(() => {}),
    },
    { label: "-", action: () => {} },
    { label: "Search", action: () => toggleSearch(true) },
    {
      label: "Export output…",
      action: () => void exportSession(tab),
    },
  ];
}

async function exportSession(tab: Tab) {
  try {
    const text = await api.exportSession(tab.sessionId);
    const blob = new Blob([text], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `alticr-${tab.sessionId}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  } catch (e) {
    showToast(`Export failed: ${e}`);
  }
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

const searchbar = $("searchbar");
const searchInput = $("search-input") as HTMLInputElement;
let searchTab: Tab | null = null;

function toggleSearch(force?: boolean) {
  const tab = tabs[activeIdx];
  if (!tab) return;
  const show = force ?? searchbar.classList.contains("hidden");
  if (show) {
    searchTab = tab;
    searchbar.classList.remove("hidden");
    searchInput.value = "";
    searchInput.focus();
    tab.search.onDidChangeResults((r) => {
      $("search-result").textContent = r.resultCount
        ? `${r.resultIndex + 1}/${r.resultCount}`
        : "";
    });
  } else {
    searchbar.classList.add("hidden");
    searchTab = null;
    tab.term.focus();
  }
}

function doSearch(next: boolean) {
  const tab = searchTab ?? tabs[activeIdx];
  if (!tab || !searchInput.value) return;
  if (next) tab.search.findNext(searchInput.value);
  else tab.search.findPrevious(searchInput.value);
}

// ---------------------------------------------------------------------------
// New tab popover
// ---------------------------------------------------------------------------

const pop = $("newtab-pop");
const ntShell = $("nt-shell") as HTMLSelectElement;

async function showNewTabPop(anchor: HTMLElement) {
  // Populate shells.
  ntShell.innerHTML = "";
  try {
    const shells = await api.detectShells();
    for (const s of shells) {
      const o = document.createElement("option");
      o.value = s.path;
      o.textContent = `${s.name} (${s.path})`;
      ntShell.appendChild(o);
    }
  } catch { /* ignore */ }
  const def = document.createElement("option");
  def.value = "";
  def.textContent = "Default shell";
  ntShell.prepend(def);

  const r = anchor.getBoundingClientRect();
  pop.classList.remove("hidden");
  pop.style.left = `${Math.max(8, r.left)}px`;
  pop.style.top = `${r.bottom + 6}px`;
  ($("nt-cwd") as HTMLInputElement).focus();
}

function hideNewTabPop() {
  pop.classList.add("hidden");
}

async function submitNewTab() {
  const shell = ntShell.value || undefined;
  const cwd = ($("nt-cwd") as HTMLInputElement).value.trim() || undefined;
  const args = ($("nt-args") as HTMLInputElement).value.trim() || undefined;
  hideNewTabPop();
  await createSession({ shell, cwd, args });
}

async function quickNewTab() {
  await createSession();
}

// ---------------------------------------------------------------------------
// Multi-window
// ---------------------------------------------------------------------------

async function newWindow() {
  const label = `w${Date.now().toString(36)}${Math.floor(Math.random() * 1e4)}`;
  try {
    const w = new WebviewWindow(label, {
      url: "/",
      title: "ALTICR",
      width: 1024,
      height: 640,
      minWidth: 480,
      minHeight: 320,
      decorations: false,
      backgroundColor: "#101418",
    });
    await w.once("tauri://created", () => {});
  } catch (e) {
    showToast(`Failed to open window: ${e}`);
  }
}

// ---------------------------------------------------------------------------
// Settings modal
// ---------------------------------------------------------------------------

const modal = $("settings-modal");
let settingsSnapshot: AppSettings | null = null;

function openSettings() {
  settingsSnapshot = JSON.parse(JSON.stringify(settings));
  // Populate fields.
  ($("set-theme") as HTMLSelectElement).value = settings.theme;
  ($("set-accent") as HTMLInputElement).value = settings.accent_color;
  ($("set-font") as HTMLInputElement).value = settings.font_family;
  ($("set-fontsize") as HTMLInputElement).value = String(settings.font_size);
  ($("set-lineheight") as HTMLInputElement).value = String(settings.line_height);
  ($("set-cursor") as HTMLSelectElement).value = settings.cursor_style;
  ($("set-blink") as HTMLInputElement).checked = settings.cursor_blink;
  ($("set-padding") as HTMLInputElement).value = String(settings.padding);
  ($("set-shell") as HTMLInputElement).value = settings.default_shell;
  ($("set-shellargs") as HTMLInputElement).value = settings.shell_args;
  ($("set-startupdir") as HTMLInputElement).value = settings.startup_dir;
  ($("set-scrollback") as HTMLInputElement).value = String(settings.scrollback_lines);
  ($("set-env") as HTMLTextAreaElement).value = Object.entries(settings.env_vars)
    .map(([k, v]) => `${k}=${v}`)
    .join("\n");
  ($("set-bell") as HTMLSelectElement).value = settings.bell;
  ($("set-tabproc") as HTMLInputElement).checked = settings.tab_show_process;
  ($("set-tabshell") as HTMLInputElement).checked = settings.tab_show_shell;
  ($("set-confirmclose") as HTMLInputElement).checked = settings.confirm_close_running;
  ($("set-osc52") as HTMLInputElement).checked = settings.allow_osc52_clipboard;
  ($("set-debug") as HTMLInputElement).checked = settings.debug_logging;
  modal.classList.remove("hidden");
}

function closeSettings() {
  modal.classList.add("hidden");
  settingsSnapshot = null;
}

function collectSettings(): Record<string, unknown> {
  const env: Record<string, string> = {};
  for (const line of ($("set-env") as HTMLTextAreaElement).value.split("\n")) {
    const eq = line.indexOf("=");
    if (eq > 0) env[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
  }
  return {
    theme: ($("set-theme") as HTMLSelectElement).value,
    accent_color: ($("set-accent") as HTMLInputElement).value,
    font_family: ($("set-font") as HTMLInputElement).value.trim(),
    font_size: parseFloat(($("set-fontsize") as HTMLInputElement).value) || 13,
    line_height: parseFloat(($("set-lineheight") as HTMLInputElement).value) || 1,
    cursor_style: ($("set-cursor") as HTMLSelectElement).value,
    cursor_blink: ($("set-blink") as HTMLInputElement).checked,
    padding: parseInt(($("set-padding") as HTMLInputElement).value) || 0,
    default_shell: ($("set-shell") as HTMLInputElement).value.trim(),
    shell_args: ($("set-shellargs") as HTMLInputElement).value.trim(),
    startup_dir: ($("set-startupdir") as HTMLInputElement).value.trim(),
    scrollback_lines: parseInt(($("set-scrollback") as HTMLInputElement).value) || 10000,
    env_vars: env,
    bell: ($("set-bell") as HTMLSelectElement).value,
    tab_show_process: ($("set-tabproc") as HTMLInputElement).checked,
    tab_show_shell: ($("set-tabshell") as HTMLInputElement).checked,
    confirm_close_running: ($("set-confirmclose") as HTMLInputElement).checked,
    allow_osc52_clipboard: ($("set-osc52") as HTMLInputElement).checked,
    debug_logging: ($("set-debug") as HTMLInputElement).checked,
  };
}

async function saveSettings() {
  try {
    const s = await api.setSettings(collectSettings());
    applySettings(s);
    closeSettings();
    showToast("Settings saved");
  } catch (e) {
    showToast(`Failed to save settings: ${e}`);
  }
}

// ---------------------------------------------------------------------------
// Keyboard shortcuts
// ---------------------------------------------------------------------------

interface Shortcut { action: string; accel: string }

function accelMatches(accel: string, e: KeyboardEvent): boolean {
  const parts = accel.toLowerCase().split("+");
  const key = parts.pop()!;
  const wantMod = parts.includes("mod");
  const wantShift = parts.includes("shift");
  const wantAlt = parts.includes("alt");
  const wantCtrl = parts.includes("ctrl");

  const mod = e.metaKey || e.ctrlKey;
  if (wantMod !== mod && !(wantCtrl && e.ctrlKey)) {
    if (wantMod ? !(e.metaKey || e.ctrlKey) : (e.metaKey || e.ctrlKey)) return false;
  }
  if (wantShift !== e.shiftKey) return false;
  if (wantAlt !== e.altKey) return false;
  if (wantCtrl && !e.ctrlKey && !e.metaKey) return false;

  const k = e.key.toLowerCase();
  if (key === "tab") return e.key === "Tab";
  if (key === "plus" || key === "+" || key === "=") {
    return k === "+" || k === "=" || e.code === "Equal" || e.code === "NumpadAdd";
  }
  if (key === "minus" || key === "-") {
    return k === "-" || k === "_" || e.code === "Minus" || e.code === "NumpadSubtract";
  }
  return k === key;
}

function inEditable(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement;
  return (
    t.tagName === "INPUT" ||
    t.tagName === "TEXTAREA" ||
    t.tagName === "SELECT" ||
    t.isContentEditable
  );
}

async function runShortcut(action: string) {
  switch (action) {
    case "new_tab":
      quickNewTab();
      break;
    case "close_tab": {
      const i = activeIdx;
      if (i >= 0) await closeTab(i);
      break;
    }
    case "new_window":
      await newWindow();
      break;
    case "next_tab":
      if (tabs.length) activateTab((activeIdx + 1) % tabs.length);
      break;
    case "prev_tab":
      if (tabs.length) activateTab((activeIdx - 1 + tabs.length) % tabs.length);
      break;
    case "copy": {
      const tab = tabs[activeIdx];
      const sel = tab?.term.getSelection();
      if (sel) await navigator.clipboard.writeText(sel).catch(() => {});
      break;
    }
    case "paste": {
      const tab = tabs[activeIdx];
      if (!tab) break;
      try {
        const t = await navigator.clipboard.readText();
        tab.term.paste(t);
      } catch { /* permission denied */ }
      break;
    }
    case "find":
      toggleSearch();
      break;
    case "font_larger":
      adjustFont(0.5);
      break;
    case "font_smaller":
      adjustFont(-0.5);
      break;
    case "font_reset":
      applyFontSize(13);
      break;
    case "select_all":
      tabs[activeIdx]?.term.selectAll();
      break;
  }
}

function adjustFont(delta: number) {
  applyFontSize(Math.min(32, Math.max(8, settings.font_size + delta)));
}

function applyFontSize(size: number) {
  settings.font_size = size;
  for (const t of tabs) t.term.options.fontSize = size;
  persistSetting({ font_size: size });
}

let persistTimer: ReturnType<typeof setTimeout> | null = null;
function persistSetting(patch: Record<string, unknown>) {
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    api.setSettings(patch).then(applySettings).catch(() => {});
  }, 400);
}

function onKeyDown(e: KeyboardEvent) {
  if (inEditable(e)) {
    if (e.key === "Escape") (e.target as HTMLElement).blur();
    return;
  }

  // Cmd/Ctrl + 1..9 tab switching.
  const mod = e.metaKey || e.ctrlKey;
  if (mod && !e.shiftKey && !e.altKey && /^[1-9]$/.test(e.key)) {
    const idx = parseInt(e.key) - 1;
    if (idx < tabs.length) {
      e.preventDefault();
      activateTab(idx);
      return;
    }
  }

  const shortcuts = settings.shortcuts || {};
  for (const [action, accel] of Object.entries(shortcuts)) {
    if (accel && accelMatches(accel, e)) {
      e.preventDefault();
      e.stopPropagation();
      void runShortcut(action);
      return;
    }
  }
}

// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

function showToast(msg: string) {
  const el = document.createElement("div");
  el.textContent = msg;
  el.style.cssText =
    "position:fixed;bottom:40px;left:50%;transform:translateX(-50%);" +
    "background:var(--bg-elev);border:1px solid var(--border);color:var(--text);" +
    "padding:8px 16px;border-radius:8px;z-index:300;font-size:12.5px;" +
    "box-shadow:0 4px 16px rgba(0,0,0,.4)";
  document.body.appendChild(el);
  setTimeout(() => el.remove(), 3000);
}

// ---------------------------------------------------------------------------
// Window controls
// ---------------------------------------------------------------------------

function wireChrome() {
  $("btn-new-tab").addEventListener("click", (e) => showNewTabPop(e.currentTarget as HTMLElement));
  $("btn-new-window").addEventListener("click", () => void newWindow());
  $("btn-settings").addEventListener("click", openSettings);
  $("win-min").addEventListener("click", () => void win.minimize());
  $("win-max").addEventListener("click", () => void win.toggleMaximize());
  $("win-close").addEventListener("click", () => void win.close());

  // Titlebar drag (decorations are disabled).
  document.querySelectorAll("[data-tauri-drag-region]").forEach((el) => {
    el.addEventListener("mousedown", (e) => {
      const me = e as MouseEvent;
      if (me.button === 0 && !(e.target as HTMLElement).closest("button")) {
        void win.startDragging();
      }
    });
    el.addEventListener("dblclick", () => void win.toggleMaximize());
  });
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

async function boot() {
  settings = await api.getSettings();
  applySettings(settings);
  wireChrome();
  wireEvents();

  // Window close confirmation for running sessions.
  await win.onCloseRequested(async (e) => {
    if (
      settings.confirm_close_running &&
      tabs.some((t) => t.state === "running" || t.state === "starting")
    ) {
      e.preventDefault();
      if (window.confirm("Some terminals still have running processes. Close anyway?")) {
        await win.destroy();
      }
    }
  });

  // Resize observer: fit all terminals (they all keep real dimensions).
  const ro = new ResizeObserver(() => {
    for (const t of tabs) scheduleFit(t);
  });
  ro.observe(termsEl);

  // Right-click on terminal surface.
  termsEl.addEventListener("contextmenu", (e) => {
    const tab = tabs[activeIdx];
    if (!tab) return;
    e.preventDefault();
    showMenu(e.clientX, e.clientY, termMenu(tab));
    tab.term.focus();
  });

  // Search bar wiring.
  $("search-close").addEventListener("click", () => toggleSearch(false));
  $("search-next").addEventListener("click", () => doSearch(true));
  $("search-prev").addEventListener("click", () => doSearch(false));
  searchInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      doSearch(e.shiftKey ? false : true);
    } else if (e.key === "Escape") {
      toggleSearch(false);
    }
  });

  // New tab popover wiring.
  $("nt-ok").addEventListener("click", () => void submitNewTab());
  $("nt-cancel").addEventListener("click", hideNewTabPop);
  pop.addEventListener("keydown", (e) => {
    if (e.key === "Enter") void submitNewTab();
    if (e.key === "Escape") hideNewTabPop();
  });
  document.addEventListener("click", (e) => {
    if (!pop.contains(e.target as Node) &&
        !(e.target as HTMLElement).closest("#btn-new-tab")) {
      hideNewTabPop();
    }
  });

  // Settings modal wiring.
  $("set-close").addEventListener("click", closeSettings);
  $("set-cancel").addEventListener("click", closeSettings);
  $("set-save").addEventListener("click", () => void saveSettings());
  $("set-reset").addEventListener("click", () => {
    if (window.confirm("Reset all settings to defaults?")) {
      api
        .setSettings({})
        .then(applySettings)
        .catch(() => {});
      // Re-open to show defaults
      closeSettings();
      openSettings();
    }
  });
  document.querySelectorAll(".set-nav-btn").forEach((b) => {
    b.addEventListener("click", () => {
      document.querySelectorAll(".set-nav-btn").forEach((x) => x.classList.remove("active"));
      document.querySelectorAll(".set-page").forEach((x) => x.classList.add("hidden"));
      b.classList.add("active");
      const sec = (b as HTMLElement).dataset.sec!;
      $(`sec-${sec}`).classList.remove("hidden");
    });
  });

  // Global shortcuts.
  window.addEventListener("keydown", onKeyDown, true);

  // Adopt sessions that may already exist for this window (e.g. restore).
  try {
    const existing = await api.listSessions(WINDOW_LABEL);
    for (const info of existing) {
      // Reattach: sessions persist only while their window lives, so this
      // is normally empty. If non-empty (e.g. hot reload), create tabs.
      void info;
    }
  } catch { /* ignore */ }

  // First tab.
  await createSession();
}

boot().catch((e) => {
  console.error("boot failed:", e);
  document.body.innerHTML = `<pre style="padding:2em;color:#f87171">ALTICR failed to start:\n${e}</pre>`;
});
