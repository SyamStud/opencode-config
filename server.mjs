import { createServer } from "node:http";
import { readFile, writeFile, copyFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CONFIG_PATH = "/home/lenovo/.config/opencode/opencode.jsonc";
const PORT = 3456;

function stripJsonComments(str) {
  let out = "";
  let inStr = false, esc = false, inSL = false, inML = false, q = "";
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
  try { return JSON.parse(stripJsonComments(raw)); } catch (e) { throw new Error("JSONC parse error: " + e.message); }
}

const mime = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".json": "application/json", ".svg": "image/svg+xml" };

const server = createServer(async (req, res) => {
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Methods", "GET,PUT,OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");
  if (req.method === "OPTIONS") { res.writeHead(204); return res.end(); }

  if (req.url === "/api/config" && req.method === "GET") {
    try {
      const cfg = await loadConfig();
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ path: CONFIG_PATH, config: cfg }));
    } catch (e) { res.writeHead(500, { "Content-Type": "application/json" }); res.end(JSON.stringify({ error: e.message })); }
    return;
  }

  if (req.url === "/api/config" && req.method === "PUT") {
    let body = "";
    for await (const c of req) body += c;
    try {
      const { config } = JSON.parse(body);
      if (!config || typeof config !== "object" || !config.provider) throw new Error("config.provider required");
      if (existsSync(CONFIG_PATH)) await copyFile(CONFIG_PATH, CONFIG_PATH + ".bak");
      await writeFile(CONFIG_PATH, JSON.stringify(config, null, 2) + "\n", "utf8");
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ ok: true }));
    } catch (e) { res.writeHead(400, { "Content-Type": "application/json" }); res.end(JSON.stringify({ error: e.message })); }
    return;
  }

  let filePath = req.url.split("?")[0];
  if (filePath === "/") filePath = "/index.html";
  const full = path.join(__dirname, "public", filePath);
  if (!full.startsWith(path.join(__dirname, "public"))) { res.writeHead(403); return res.end(); }
  try {
    const data = await readFile(full);
    const ext = path.extname(full);
    res.writeHead(200, { "Content-Type": mime[ext] || "text/plain" });
    res.end(data);
  } catch { res.writeHead(404); res.end("Not found"); }
});

server.listen(PORT, () => {
  console.log(`openconfig editor running at http://localhost:${PORT}`);
  console.log(`editing: ${CONFIG_PATH}  (WSL: \\\\wsl.localhost\\Ubuntu${CONFIG_PATH})`);
});
