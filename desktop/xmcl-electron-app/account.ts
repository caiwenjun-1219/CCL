/**
 * CCL 账号的 IPC 通道与共享类型。
 *
 * 账号服务与安卓端同一个后端（zh.ccl.caiwenjun.top）。
 * 除了账号本身，这里还承载三件与账号绑定的能力：
 * - 把账号当作 Minecraft 认证服务器（Yggdrasil / authlib-injector）
 * - 启动器偏好设置的云端同步
 * - Campfire 好友房间目录
 */

export const CCL_ACCOUNT_BASE = 'https://zh.ccl.caiwenjun.top'

/**
 * CCL 的 Yggdrasil API 根地址。
 * 与 LittleSkin 的 `https://littleskin.cn/api/yggdrasil` 对齐，
 * 可以直接作为启动器里的第三方认证服务器添加。
 */
export const CCL_YGGDRASIL = `${CCL_ACCOUNT_BASE}/api/yggdrasil`

export interface CclAccountUser {
  username: string
  nickname: string
  email: string
  emailVerified: boolean
  /** 是否已设置登录密码（邮箱验证码注册的账号可能没有） */
  hasPassword?: boolean
  nicknameStatus: string
  mcName?: string | null
  mcUuid?: string | null
  mcVerified?: boolean | null
  skinUrl?: string | null
  createdAt?: number
}

export interface CclAccountState {
  user: CclAccountUser | null
  /** 本地是否还有可用的刷新令牌（用于界面提示「会话已过期」） */
  expired?: boolean
}

export type CclResult<T> = { ok: true; data: T } | { ok: false; code: string; message?: string }

export interface CclLoginResult {
  twoFactorRequired: boolean
  challengeToken?: string
}

export interface CclRoom {
  id: string
  name: string
  owner: string
  /** Campfire 房间码 */
  code: string
  game_port: number
  version?: string | null
  motd?: string | null
  players: number
  max_players: number
  mods: string[]
  created_at: number
  updated_at: number
}

export interface CclRoomPayload {
  name?: string
  code: string
  game_port?: number
  version?: string
  motd?: string
  players?: number
  max_players?: number
  mods?: string[]
}

export interface CclSyncResult {
  /** 参与同步的键数量 */
  keys: number
  /** 下载时真正变化的键 */
  changed: string[]
  /** 本地配置文件备份路径（仅下载时有值） */
  backup?: string
}

export interface CclAccountBridge {
  getState(): Promise<CclAccountState>
  requestEmailCode(email: string): Promise<CclResult<{ cooldown: number; expiresIn: number }>>
  loginWithEmailCode(email: string, code: string): Promise<CclResult<CclLoginResult>>
  loginWithPassword(login: string, password: string): Promise<CclResult<CclLoginResult>>
  verifyTwoFactor(challengeToken: string, code: string): Promise<CclResult<null>>
  refresh(): Promise<CclResult<CclAccountState>>
  logout(): Promise<CclResult<null>>
  openRegister(): Promise<void>

  changePassword(oldPassword: string, newPassword: string): Promise<CclResult<null>>
  bindMinecraft(name: string, uuid: string): Promise<CclResult<CclAccountUser>>
  unbindMinecraft(): Promise<CclResult<CclAccountUser>>

  syncUpload(): Promise<CclResult<CclSyncResult>>
  syncDownload(): Promise<CclResult<CclSyncResult>>

  listRooms(): Promise<CclResult<CclRoom[]>>
  publishRoom(payload: CclRoomPayload): Promise<CclResult<CclRoom>>
  heartbeatRoom(id: string, players?: number): Promise<CclResult<CclRoom>>
  removeRoom(id: string): Promise<CclResult<null>>

  onState(listener: (state: CclAccountState) => void): () => void
}

export const cclAccountChannels = {
  getState: 'ccl-account:get-state',
  requestEmailCode: 'ccl-account:request-email-code',
  loginWithEmailCode: 'ccl-account:login-email-code',
  loginWithPassword: 'ccl-account:login-password',
  verifyTwoFactor: 'ccl-account:verify-2fa',
  refresh: 'ccl-account:refresh',
  logout: 'ccl-account:logout',
  openRegister: 'ccl-account:open-register',
  changePassword: 'ccl-account:change-password',
  bindMinecraft: 'ccl-account:bind-minecraft',
  unbindMinecraft: 'ccl-account:unbind-minecraft',
  syncUpload: 'ccl-account:sync-upload',
  syncDownload: 'ccl-account:sync-download',
  listRooms: 'ccl-account:list-rooms',
  publishRoom: 'ccl-account:publish-room',
  heartbeatRoom: 'ccl-account:heartbeat-room',
  removeRoom: 'ccl-account:remove-room',
  state: 'ccl-account:state',
} as const
