import { ref, shallowRef } from 'vue'

export const CCL_ACCOUNT_BASE = 'https://zh.ccl.caiwenjun.top'
/** CCL 的 Yggdrasil API 根，可直接作为第三方认证服务器添加到启动器。 */
export const CCL_YGGDRASIL = `${CCL_ACCOUNT_BASE}/api/yggdrasil`

export interface CclAccountUser {
  username: string
  nickname: string
  email: string
  emailVerified: boolean
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
  keys: number
  changed: string[]
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

const bridge = typeof cclAccount !== 'undefined' ? cclAccount : undefined

const user = shallowRef<CclAccountUser | null>(null)
const expired = ref(false)
let wired = false

function ensureWired() {
  if (wired || !bridge) return
  wired = true
  bridge.getState().then((state) => {
    user.value = state.user
    expired.value = !!state.expired
  }).catch(() => undefined)
  bridge.onState((state) => {
    user.value = state.user
    expired.value = !!state.expired
  })
}

/** 错误码 → 多语言 key，视图里 `t(key)` 即可。 */
export function cclErrorKey(code: string): string {
  const known = [
    'invalid_credentials',
    'invalid_email',
    'invalid_code',
    'code_expired',
    'code_cooldown',
    'code_attempts_exceeded',
    'rate_limited',
    'mail_failed',
    'not_logged_in',
    'network_error',
    'invalid_two_factor_code',
    'challenge_expired',
    'invalid_minecraft_name',
    'invalid_minecraft_uuid',
    'nothing_to_sync',
    'invalid_room_code',
    'invalid_game_port',
    'room_not_found',
    'not_room_owner',
  ]
  return known.includes(code) ? `cclAccount.error.${code}` : 'cclAccount.error.unknown'
}

export function useCclAccount() {
  ensureWired()
  return {
    available: !!bridge,
    user,
    expired,
    requestEmailCode: (email: string) => bridge!.requestEmailCode(email),
    loginWithEmailCode: (email: string, code: string) => bridge!.loginWithEmailCode(email, code),
    loginWithPassword: (login: string, password: string) => bridge!.loginWithPassword(login, password),
    verifyTwoFactor: (challenge: string, code: string) => bridge!.verifyTwoFactor(challenge, code),
    refresh: () => bridge!.refresh(),
    logout: () => bridge!.logout(),
    openRegister: () => bridge!.openRegister(),
    changePassword: (oldPassword: string, newPassword: string) => bridge!.changePassword(oldPassword, newPassword),
    bindMinecraft: (name: string, uuid: string) => bridge!.bindMinecraft(name, uuid),
    unbindMinecraft: () => bridge!.unbindMinecraft(),
    syncUpload: () => bridge!.syncUpload(),
    syncDownload: () => bridge!.syncDownload(),
    listRooms: () => bridge!.listRooms(),
    publishRoom: (payload: CclRoomPayload) => bridge!.publishRoom(payload),
    heartbeatRoom: (id: string, players?: number) => bridge!.heartbeatRoom(id, players),
    removeRoom: (id: string) => bridge!.removeRoom(id),
  }
}
