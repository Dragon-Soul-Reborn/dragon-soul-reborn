import "./style.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, type DisplayOptions, type Info, type Settings, type Status } from "./api";
import { bytes, progressLine, SpeedMeter } from "./format";

const el = <T extends HTMLElement = HTMLElement>(id: string): T => {
  const node = document.getElementById(id);
  if (!node) throw new Error(`missing #${id}`);
  return node as T;
};
const input = (id: string) => el<HTMLInputElement>(id);

let status: Status = { kind: "starting" };
let info: Info | null = null;
let serverUp: boolean | null = null;
let showingSettings = false;
let installCopy: string | null = null;
let installAnyway = false;
let lastStage = "";
let flashTimer: number | undefined;
let spaceTimer: number | undefined;
const meter = new SpeedMeter();

function render(): void {
  el("settings").hidden = !showingSettings;
  el("install").hidden = showingSettings || status.kind !== "needsFolder";
  const message = el("message");
  message.hidden = showingSettings || status.kind === "needsFolder" || !info?.message;
  message.textContent = info?.message ? `“${info.message}”` : "";
  renderServer();
  renderFooter();
}

function renderServer(): void {
  const s = el("server");
  s.className = `server ${serverUp === null ? "unknown" : serverUp ? "up" : "down"}`;
  s.textContent = serverUp === null ? "● Server status unknown" : serverUp ? "● Server online" : "● Server offline";
}

function renderFooter(): void {
  const text = el("status-text");
  const bar = el("bar");
  const action = el<HTMLButtonElement>("action-btn");
  text.classList.remove("error");
  action.classList.remove("stop");
  let fraction: number | null = 0;
  let label = "PLAY";
  let enabled = false;
  switch (status.kind) {
    case "starting":
      text.textContent = "Starting…";
      fraction = null;
      break;
    case "updatingLauncher":
      text.textContent = "Updating the launcher…";
      fraction = null;
      break;
    case "needsFolder":
      text.textContent = "Choose where to install the game.";
      label = "INSTALL";
      enabled = true;
      break;
    case "working": {
      const p = status.progress;
      if (p.stage !== lastStage) {
        meter.reset();
        lastStage = p.stage;
      }
      if (p.stage === "downloading") meter.push(Date.now(), p.done);
      const line = progressLine(p, meter.bytesPerSecond());
      text.textContent = line.text;
      fraction = line.fraction;
      label = "STOP";
      enabled = true;
      action.classList.add("stop");
      break;
    }
    case "ready":
      text.textContent = (status.offline ? "Couldn't check for updates. Playing the installed version." : "Game is up to date.") + (status.note ? ` ${status.note}` : "");
      fraction = 1;
      enabled = true;
      break;
    case "error":
      text.textContent = status.message;
      text.classList.add("error");
      label = "RETRY";
      enabled = true;
      break;
  }
  bar.classList.toggle("indeterminate", fraction === null);
  el("bar-fill").style.width = fraction === null ? "30%" : `${Math.round(fraction * 100)}%`;
  action.textContent = label;
  action.disabled = !enabled || showingSettings;
  el<HTMLButtonElement>("settings-btn").disabled = ["starting", "updatingLauncher", "working"].includes(status.kind);
}

function flash(message: string): void {
  const text = el("status-text");
  text.textContent = message;
  text.classList.add("error");
  window.clearTimeout(flashTimer);
  flashTimer = window.setTimeout(renderFooter, 8000);
}

async function onAction(): Promise<void> {
  switch (status.kind) {
    case "needsFolder":
      return startInstall();
    case "working":
      return api.cancel();
    case "ready":
      return api.play().catch((e) => flash(String(e)));
    case "error":
      return api.retry();
  }
}

async function startInstall(): Promise<void> {
  const folder = input("install-folder").value.trim();
  if (!folder) return flash("Choose a folder first.");
  const check = await api.checkFolder(folder);
  if (check.exists && !check.empty && !check.hasGame && !installAnyway) {
    el("install-sub").textContent = `${folder.replace(/[\\/]+$/, "")}\\Dragon Soul`;
    el("install-prompt").hidden = false;
    return;
  }
  el("install-prompt").hidden = true;
  await api.install(folder, installCopy);
}

function updateSpace(): void {
  window.clearTimeout(spaceTimer);
  spaceTimer = window.setTimeout(async () => {
    const folder = input("install-folder").value.trim();
    const line = el("install-space");
    installAnyway = false;
    el("install-prompt").hidden = true;
    if (!folder) {
      line.textContent = "";
      return;
    }
    const c = await api.checkFolder(folder);
    line.textContent = `About ${bytes(c.neededBytes)} needed while installing, ${c.freeBytes === null ? "free space unknown" : `${bytes(c.freeBytes)} free`}.`;
    line.classList.toggle("error", c.freeBytes !== null && c.freeBytes < c.neededBytes);
  }, 300);
}

function fillSettings(s: Settings, d: DisplayOptions): void {
  const select = el<HTMLSelectElement>("set-res");
  select.replaceChildren();
  const add = (value: string, label: string) => {
    const o = document.createElement("option");
    o.value = value;
    o.textContent = label;
    select.append(o);
  };
  add("auto", `Auto (recommended): ${d.auto.width}×${d.auto.height}`);
  add("native", `Native: ${d.native.width}×${d.native.height}`);
  for (const m of d.modes) add(`fixed:${m.width}x${m.height}`, `${m.width}×${m.height}`);
  const r = s.resolution;
  const want = r.kind === "fixed" ? `fixed:${r.width}x${r.height}` : r.kind;
  if (r.kind === "fixed" && ![...select.options].some((o) => o.value === want)) add(want, `${r.width}×${r.height}`);
  select.value = want;
  input("set-full").checked = s.fullscreen;
  input("set-window").checked = !s.fullscreen;
  input("set-folder").value = s.gameFolder ?? "";
  input("set-server").value = s.serverOverride ?? "";
}

function readSettings(): Settings {
  const v = el<HTMLSelectElement>("set-res").value;
  const fixed = /^fixed:(\d+)x(\d+)$/.exec(v);
  const resolution: Settings["resolution"] = fixed
    ? { kind: "fixed", width: Number(fixed[1]), height: Number(fixed[2]) }
    : v === "native"
      ? { kind: "native" }
      : { kind: "auto" };
  return {
    gameFolder: input("set-folder").value.trim() || null,
    resolution,
    fullscreen: input("set-full").checked,
    serverOverride: input("set-server").value.trim() || null,
  };
}

async function openSettings(): Promise<void> {
  const [settings, display] = await Promise.all([api.settings(), api.displayOptions()]);
  fillSettings(settings, display);
  el("set-error").hidden = true;
  showingSettings = true;
  render();
}

async function saveSettings(): Promise<void> {
  try {
    await api.saveSettings(readSettings());
    showingSettings = false;
    render();
    void pollServer();
  } catch (e) {
    const err = el("set-error");
    err.textContent = String(e);
    err.hidden = false;
  }
}

async function refreshInfo(): Promise<void> {
  info = await api.info();
  render();
  void pollServer();
}

async function pollServer(): Promise<void> {
  try {
    serverUp = await api.serverStatus();
  } catch {
    serverUp = null;
  }
  renderServer();
}

function suggestFolder(s: Status): void {
  if (s.kind === "needsFolder" && !input("install-folder").value) {
    input("install-folder").value = s.suggested;
    updateSpace();
  }
}

function wire(): void {
  const win = getCurrentWindow();
  el("tb-min").addEventListener("click", () => void win.minimize());
  el("tb-close").addEventListener("click", () => void win.close());
  el("action-btn").addEventListener("click", () => void onAction());
  el("settings-btn").addEventListener("click", () => void openSettings());
  el("install-folder").addEventListener("input", updateSpace);
  el("install-browse").addEventListener("click", async () => {
    const f = await api.pickFolder();
    if (f) {
      input("install-folder").value = f;
      updateSpace();
    }
  });
  el("install-copy").addEventListener("click", async () => {
    installCopy = await api.pickInstaller();
    el("install-copy-name").textContent = installCopy ?? "";
  });
  el("install-sub-yes").addEventListener("click", () => {
    input("install-folder").value = el("install-sub").textContent ?? "";
    void startInstall();
  });
  el("install-here").addEventListener("click", () => {
    installAnyway = true;
    void startInstall();
  });
  el("set-folder-change").addEventListener("click", async () => {
    const f = await api.pickFolder();
    if (f) input("set-folder").value = f;
  });
  el("set-repair").addEventListener("click", () => {
    showingSettings = false;
    render();
    void api.repair();
  });
  el("set-logs").addEventListener("click", () => void api.openLogs());
  el("set-cancel").addEventListener("click", () => {
    showingSettings = false;
    render();
  });
  el("set-save").addEventListener("click", () => void saveSettings());
}

async function main(): Promise<void> {
  wire();
  await api.onStatus((s) => {
    const wasReady = status.kind === "ready";
    status = s;
    suggestFolder(s);
    render();
    if (s.kind === "ready" && !wasReady) void refreshInfo();
  });
  status = await api.status();
  suggestFolder(status);
  await refreshInfo();
  window.setInterval(() => void pollServer(), 30_000);
}

void main();
