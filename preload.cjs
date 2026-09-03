const { contextBridge, ipcRenderer } = require("electron");
contextBridge.exposeInMainWorld("api", {
  load: () => ipcRenderer.invoke("load-config"),
  save: (config) => ipcRenderer.invoke("save-config", config),
  browse: () => ipcRenderer.invoke("browse-config"),
  searchConfigs: () => ipcRenderer.invoke("search-configs"),
  useConfig: (p) => ipcRenderer.invoke("use-config", p),
  fetchModels: (p) => ipcRenderer.invoke("fetch-models", p),
  close: () => ipcRenderer.send("win-close"),
  minimize: () => ipcRenderer.send("win-minimize"),
  maximize: () => ipcRenderer.send("win-maximize"),
  isMaximized: () => ipcRenderer.invoke("win-is-maximized")
});
