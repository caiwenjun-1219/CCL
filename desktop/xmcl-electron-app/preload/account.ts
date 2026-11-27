import { contextBridge, ipcRenderer } from 'electron/renderer'
import { cclAccountChannels, type CclAccountBridge, type CclAccountState } from '../account'

const cclAccount: CclAccountBridge = {
  getState: () => ipcRenderer.invoke(cclAccountChannels.getState),
  requestEmailCode: (email) => ipcRenderer.invoke(cclAccountChannels.requestEmailCode, email),
  loginWithEmailCode: (email, code) => ipcRenderer.invoke(cclAccountChannels.loginWithEmailCode, email, code),
  loginWithPassword: (login, password) => ipcRenderer.invoke(cclAccountChannels.loginWithPassword, login, password),
  verifyTwoFactor: (challenge, code) => ipcRenderer.invoke(cclAccountChannels.verifyTwoFactor, challenge, code),
  refresh: () => ipcRenderer.invoke(cclAccountChannels.refresh),
  logout: () => ipcRenderer.invoke(cclAccountChannels.logout),
  openRegister: () => ipcRenderer.invoke(cclAccountChannels.openRegister),
  changePassword: (oldPassword, newPassword) =>
    ipcRenderer.invoke(cclAccountChannels.changePassword, oldPassword, newPassword),
  bindMinecraft: (name, uuid) => ipcRenderer.invoke(cclAccountChannels.bindMinecraft, name, uuid),
  unbindMinecraft: () => ipcRenderer.invoke(cclAccountChannels.unbindMinecraft),
  syncUpload: () => ipcRenderer.invoke(cclAccountChannels.syncUpload),
  syncDownload: () => ipcRenderer.invoke(cclAccountChannels.syncDownload),
  listRooms: () => ipcRenderer.invoke(cclAccountChannels.listRooms),
  publishRoom: (payload) => ipcRenderer.invoke(cclAccountChannels.publishRoom, payload),
  heartbeatRoom: (id, players) => ipcRenderer.invoke(cclAccountChannels.heartbeatRoom, id, players),
  removeRoom: (id) => ipcRenderer.invoke(cclAccountChannels.removeRoom, id),
  onState: (listener) => {
    const handler = (_event: unknown, state: CclAccountState) => listener(state)
    ipcRenderer.on(cclAccountChannels.state, handler)
    return () => ipcRenderer.off(cclAccountChannels.state, handler)
  },
}

contextBridge.exposeInMainWorld('cclAccount', cclAccount)
