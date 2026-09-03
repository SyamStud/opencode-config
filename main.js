import { app, BrowserWindow, ipcMain, Menu, dialog } from "electron";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { readFile, writeFile, copyFile } from "node:fs/promises";
import { existsSync, statSync } from "node:fs";
import os from "node:os";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
let CONFIG_PATH = autoSearchConfig();

function autoSearchConfig() {
  const home = os.homedir();
  const xdg = process.env.XDG_CONFIG_HOME || path.join(home, ".config");
  const candidates = [
    path.join(xdg, "opencode", "opencode.jsonc"),
    path.join(xdg, "opencode", "opencode.json"),
    path.join(home, ".config", "opencode", "opencode.jsonc"),
    path.join(home, ".config", "opencode", "opencode.json"),
    path.join(process.cwd(), "opencode.jsonc"),
    path.join(process.cwd(), "opencode.json"),
    "/home/lenovo/.config/opencode/opencode.jsonc",
    "/home/lenovo/.config/opencode/opencode.json",
  ];
  const uniq = [...new Set(candidates)];
  let best = null, bestMtime = 0;
  for (const p of uniq) {
    try { if (existsSync(p)) {
      const s = statSync(p);
      const raw = s.isFile() ? s.mtimeMs : 0;
      if (raw > bestMtime) { best = p; bestMtime = raw; }
    }} catch {}
  }
  if (best) return best;
  return path.join(xdg, "opencode", "opencode.jsonc");
}
function wslPath(p){ return `\\\\wsl.localhost\\Ubuntu${p}`; }

function stripJsonComments(str) {
  let out = "", inStr = false, esc = false, inSL = false, inML = false, q = "";
  for (let i = 0; i < str.length; i++) {
    const c = str[i], n = str[i + 1];
    if (inSL) { if (c === "\n") { inSL = false; out += c; } continue; }
    if (inML) { if (c === "*" && n === "/") { inML = false; i++; } continue; }
    if (inStr) {
      out += c;
      if (esc) esc = false;
      else if (c === "\\") esc = true;
      else if (c === q) inStr = false;
      continue;
    }
    if (c === '"' || c === "'") { inStr = true; q = c; out += c; continue; }
    if (c === "/" && n === "/") { inSL = true; i++; continue; }
    if (c === "/" && n === "*") { inML = true; i++; continue; }
    out += c;
  }
  return out;
}
async function loadConfig() {
  if (!existsSync(CONFIG_PATH)) return { $schema: "https://opencode.ai/config.json", provider: {} };
  const raw = await readFile(CONFIG_PATH, "utf8");
  return JSON.parse(stripJsonComments(raw));
}

ipcMain.handle("load-config", async () => {
  const config = await loadConfig();
  return { path: CONFIG_PATH, wslPath: wslPath(CONFIG_PATH), config, candidates: autoSearchConfig() };
});
ipcMain.handle("save-config", async (_e, config) => {
  if (!config?.provider) throw new Error("provider required");
  await writeFile(CONFIG_PATH, JSON.stringify(config, null, 2) + "\n", "utf8");
  return { ok: true };
});
ipcMain.handle("browse-config", async () => {
  const { canceled, filePaths } = await dialog.showOpenDialog({
    title: "Pilih opencode.json / opencode.jsonc",
    filters: [{ name: "opencode config", extensions: ["jsonc","json"] }],
    properties: ["openFile"]
  });
  if (canceled || !filePaths[0]) return null;
  CONFIG_PATH = filePaths[0];
  const config = await loadConfig();
  return { path: CONFIG_PATH, wslPath: wslPath(CONFIG_PATH), config };
});
ipcMain.handle("search-configs", async () => {
  const home = os.homedir();
  const xdg = process.env.XDG_CONFIG_HOME || path.join(home, ".config");
  const candidates = [...new Set([
    path.join(xdg, "opencode", "opencode.jsonc"),
    path.join(xdg, "opencode", "opencode.json"),
    path.join(home, ".config", "opencode", "opencode.jsonc"),
    path.join(home, ".config", "opencode", "opencode.json"),
    path.join(process.cwd(), "opencode.jsonc"),
    path.join(process.cwd(), "opencode.json"),
  ])];
  return candidates.filter(p=> { try{ return existsSync(p) } catch{ return false } }).map(p=> {
    try{ const s=statSync(p); return { path:p, wslPath:wslPath(p), mtime:s.mtimeMs, size:s.size } } catch { return { path:p, wslPath:wslPath(p) } }
  });
});
ipcMain.handle("use-config", async (_e, p) => {
  CONFIG_PATH = p;
  const config = await loadConfig();
  return { path: CONFIG_PATH, wslPath: wslPath(CONFIG_PATH), config };
});
ipcMain.on("win-close", () => BrowserWindow.getFocusedWindow()?.close());
ipcMain.on("win-minimize", () => BrowserWindow.getFocusedWindow()?.minimize());
ipcMain.on("win-maximize", () => {
  const w = BrowserWindow.getFocusedWindow();
  if (!w) return;
  if (w.isMaximized()) w.unmaximize();
  else w.maximize();
});
ipcMain.handle("win-is-maximized", () => BrowserWindow.getFocusedWindow()?.isMaximized() ?? false);
ipcMain.handle("fetch-models", async (_e, { baseURL, apiKey }) => {
  if (!baseURL) throw new Error("baseURL kosong");
  const url = baseURL.replace(/\/$/, "") + "/models";
  const headers = {};
  if (apiKey) headers["Authorization"] = `Bearer ${apiKey}`;
  const r = await fetch(url, { headers });
  if (!r.ok) throw new Error(`${r.status} ${r.statusText} - ${await r.text().catch(()=>"")}`);
  const j = await r.json();
  const list = Array.isArray(j) ? j : j.data || j.models || [];
  return list.map(m => typeof m === "string" ? m : m.id || m.name || JSON.stringify(m)).filter(Boolean);
});

app.disableHardwareAcceleration();
app.commandLine.appendSwitch("disable-gpu");
app.commandLine.appendSwitch("disable-gpu-compositing");
Menu.setApplicationMenu(null);

function createWindow() {
  const win = new BrowserWindow({
    width: 1100,
    height: 800,
    center: true,
    frame: true,
    autoHideMenuBar: true,
    backgroundColor: "#f9fafb",
    title: "openconfig",
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  });
  win.loadFile(path.join(__dirname, "public", "index.html"));
}

app.whenReady().then(createWindow);
app.on("window-all-closed", () => { if (process.platform !== "darwin") app.quit(); });
app.on("activate", () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); });
