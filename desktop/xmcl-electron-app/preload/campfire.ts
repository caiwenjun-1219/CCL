import type { LinkState } from '@ccl/campfire-link'
import { contextBridge, ipcRenderer } from 'electron/renderer'
import { campfireChannels, type CampfireBridge } from '../campfire'

const campfire: CampfireBridge = {
  startHost: (options) => ipcRenderer.invoke(campfireChannels.startHost, options),
  startGuest: (options) => ipcRenderer.invoke(campfireChannels.startGuest, options),
  stop: () => ipcRenderer.invoke(campfireChannels.stop),
  getState: () => ipcRenderer.invoke(campfireChannels.getState),
  getConfig: () => ipcRenderer.invoke(campfireChannels.getConfig),
  pickExecutable: () => ipcRenderer.invoke(campfireChannels.pickExecutable),
  openFull: () => ipcRenderer.invoke(campfireChannels.openFull),
  onState: (listener) => {
    const handler = (_event: unknown, state: LinkState) => listener(state)
    ipcRenderer.on(campfireChannels.state, handler)
    return () => ipcRenderer.off(campfireChannels.state, handler)
  },
}

contextBridge.exposeInMainWorld('campfire', campfire)
