import { contextBridge, ipcRenderer } from "electron";
contextBridge.exposeInMainWorld("api", {
  load: () => ipcRenderer.invoke("load-config"),
  save: (config) => ipcRenderer.invoke("save-config", config)
});
