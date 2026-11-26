import { type IpcMain, type WebContents, app, safeStorage, shell, webContents } from 'electron'
import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  CCL_ACCOUNT_BASE,
  cclAccountChannels,
  type CclAccountState,
  type CclAccountUser,
  type CclLoginResult,
  type CclResult,
  type CclRoom,
  type CclRoomPayload,
  type CclSyncResult,
} from '../account'

/**
 * 参与云端同步的启动器设置。
 * 刻意不含 globalJava / httpProxy（机器相关或含密码）与所有运行时状态字段。
 */
const SYNC_KEYS = [
  'locale',
  'theme',
  'globalMinMemory',
  'globalMaxMemory',
  'globalAssignMemory',
  'globalPreferredJavaVendor',
  'globalMcOptions',
  'globalFastLaunch',
  'globalHideLauncher',
  'globalShowLog',
  'globalSyncGameLanguage',
  'globalDisableAuthlibInjector',
  'globalDisableElyByAuthlib',
  'globalPreExecuteCommand',
  'globalEnv',
  'globalResolution',
  'developerMode',
  'disableTelemetry',
  'multiplayerTransport',
  'replaceNatives',
  'enableDedicatedGPUOptimization',
  'allowPrerelease',
  'autoDownload',
  'autoInstallOnAppQuit',
  'discordPresence',
  'windowTranslucent',
  'linuxTitlebar',
  'quickActionShortcut',
  'apiSetsPreference',
  'apiSets',
] as const

const SYNC_PREFIX = 'setting:'

interface Session {
  accessToken: string
  refreshToken: string
  /** 秒级时间戳 */
  expiresAt: number
  user: CclAccountUser
}

interface UserPayload {
  username?: string
  nickname?: string
  email?: string
  email_verified?: boolean
  has_password?: boolean
  nickname_status?: string
  mc_name?: string | null
  mc_uuid?: string | null
  mc_verified?: boolean | null
  skin_url?: string | null
  created_at?: number
}

function toUser(payload: UserPayload | undefined): CclAccountUser {
  return {
    username: payload?.username ?? '',
    nickname: payload?.nickname || payload?.username || '',
    email: payload?.email ?? '',
    emailVerified: payload?.email_verified === true,
    hasPassword: payload?.has_password === true,
    nicknameStatus: payload?.nickname_status ?? 'ok',
    mcName: payload?.mc_name ?? null,
    mcUuid: payload?.mc_uuid ?? null,
    mcVerified: payload?.mc_verified ?? null,
    skinUrl: payload?.skin_url ?? null,
    createdAt: payload?.created_at,
  }
}

/**
 * 桌面端 CCL 账号控制器。
 *
 * - 令牌只留在主进程，渲染层拿不到；落盘用 safeStorage 加密。
 * - 访问令牌 2 小时，过期或收到 401 时自动用刷新令牌续期一次。
 * - 网络错误与业务错误都以 `{ ok:false, code }` 返回，渲染层据此做本地化。
 */
export class AccountController {
  private readonly owners = new Set<number>()
  private session: Session | undefined

  constructor(private readonly ipc: IpcMain) {
    this.session = this.load()

    ipc.handle(cclAccountChannels.getState, async () => this.state())
    ipc.handle(cclAccountChannels.requestEmailCode, async (_e, email: string) => this.requestEmailCode(email))
    ipc.handle(cclAccountChannels.loginWithEmailCode, async (_e, email: string, code: string) =>
      this.loginWithEmailCode(email, code),
    )
    ipc.handle(cclAccountChannels.loginWithPassword, async (_e, login: string, password: string) =>
      this.loginWithPassword(login, password),
    )
    ipc.handle(cclAccountChannels.verifyTwoFactor, async (_e, challenge: string, code: string) =>
      this.verifyTwoFactor(challenge, code),
    )
    ipc.handle(cclAccountChannels.refresh, async () => this.refresh())
    ipc.handle(cclAccountChannels.logout, async () => this.logout())
    ipc.handle(cclAccountChannels.openRegister, async () => {
      await shell.openExternal(CCL_ACCOUNT_BASE)
    })

    ipc.handle(cclAccountChannels.changePassword, async (_e, oldPassword: string, newPassword: string) =>
      this.changePassword(oldPassword, newPassword),
    )
    ipc.handle(cclAccountChannels.bindMinecraft, async (_e, name: string, uuid: string) =>
      this.bindMinecraft(name, uuid),
    )
    ipc.handle(cclAccountChannels.unbindMinecraft, async () => this.unbindMinecraft())
    ipc.handle(cclAccountChannels.syncUpload, async () => this.syncUpload())
    ipc.handle(cclAccountChannels.syncDownload, async () => this.syncDownload())
    ipc.handle(cclAccountChannels.listRooms, async () => this.listRooms())
    ipc.handle(cclAccountChannels.publishRoom, async (_e, payload: CclRoomPayload) => this.publishRoom(payload))
    ipc.handle(cclAccountChannels.heartbeatRoom, async (_e, id: string, players?: number) =>
      this.heartbeatRoom(id, players),
    )
    ipc.handle(cclAccountChannels.removeRoom, async (_e, id: string) => this.removeRoom(id))
  }

  attach(sender: WebContents) {
    this.owners.add(sender.id)
    sender.send(cclAccountChannels.state, this.state())
  }

  detach(id: number) {
    this.owners.delete(id)
  }

  private get file() {
    return join(app.getPath('userData'), 'ccl-account.json')
  }

  private load(): Session | undefined {
    try {
      if (!existsSync(this.file)) return undefined
      const raw = readFileSync(this.file)
      const text = safeStorage.isEncryptionAvailable()
        ? safeStorage.decryptString(Buffer.from(raw.toString('utf8'), 'base64'))
        : raw.toString('utf8')
      const parsed = JSON.parse(text) as Session
      if (!parsed?.accessToken || !parsed?.user) return undefined
      return parsed
    } catch (error) {
      console.warn('[ccl-account] 读取本地会话失败', error)
      return undefined
    }
  }

  private save(session: Session | undefined) {
    this.session = session
    try {
      if (!session) {
        if (existsSync(this.file)) rmSync(this.file, { force: true })
        return
      }
      const text = JSON.stringify(session)
      const data = safeStorage.isEncryptionAvailable()
        ? safeStorage.encryptString(text).toString('base64')
        : text
      writeFileSync(this.file, data, { mode: 0o600 })
    } catch (error) {
      console.warn('[ccl-account] 保存本地会话失败', error)
    }
  }

  private state(): CclAccountState {
    return { user: this.session?.user ?? null, expired: !!this.session && this.session.expiresAt * 1000 < Date.now() }
  }

  private broadcast() {
    const state = this.state()
    for (const id of this.owners) {
      const contents = webContents.fromId(id)
      if (contents && !contents.isDestroyed()) contents.send(cclAccountChannels.state, state)
    }
  }

  private async api(
    path: string,
    init: { method?: string; body?: unknown; token?: string } = {},
  ): Promise<{ status: number; json: any }> {
    const response = await fetch(`${CCL_ACCOUNT_BASE}/api/v1${path}`, {
      method: init.method ?? (init.body === undefined ? 'GET' : 'POST'),
      headers: {
        Accept: 'application/json',
        ...(init.body === undefined ? {} : { 'Content-Type': 'application/json' }),
        ...(init.token ? { Authorization: `Bearer ${init.token}` } : {}),
      },
      body: init.body === undefined ? undefined : JSON.stringify(init.body),
      signal: AbortSignal.timeout(20_000),
    })
    let json: any = undefined
    try {
      json = await response.json()
    } catch {
      json = undefined
    }
    return { status: response.status, json }
  }

  private failure(status: number, json: any): { ok: false; code: string; message?: string } {
    const code = json?.error?.code ?? (status === 429 ? 'rate_limited' : 'request_failed')
    return { ok: false, code, message: json?.error?.message }
  }

  private async applySession(data: any): Promise<CclLoginResult> {
    if (data?.two_factor_required) {
      const challengeToken = String(data.challenge_token ?? '')
      if (!challengeToken) throw new Error('invalid_response')
      return { twoFactorRequired: true, challengeToken }
    }
    const tokens = data?.tokens
    const user = data?.user
    if (!tokens?.access_token || !user) throw new Error('invalid_response')
    this.save({
      accessToken: tokens.access_token,
      refreshToken: tokens.refresh_token ?? '',
      expiresAt: Math.floor(Date.now() / 1000) + Number(tokens.expires_in ?? 7200),
      user: toUser(user),
    })
    this.broadcast()
    return { twoFactorRequired: false }
  }

  private async requestEmailCode(email: string): Promise<CclResult<{ cooldown: number; expiresIn: number }>> {
    try {
      const { status, json } = await this.api('/auth/email/code', { body: { email } })
      if (status === 200 && json?.ok) {
        return {
          ok: true,
          data: { cooldown: Number(json.data?.cooldown ?? 60), expiresIn: Number(json.data?.expires_in ?? 600) },
        }
      }
      return this.failure(status, json)
    } catch (error) {
      console.warn('[ccl-account] requestEmailCode', error)
      return { ok: false, code: 'network_error' }
    }
  }

  private async loginWithEmailCode(email: string, code: string): Promise<CclResult<CclLoginResult>> {
    try {
      const { status, json } = await this.api('/auth/email/verify', { body: { email, code } })
      if (status === 200 && json?.ok) return { ok: true, data: await this.applySession(json.data) }
      return this.failure(status, json)
    } catch (error) {
      console.warn('[ccl-account] loginWithEmailCode', error)
      return { ok: false, code: 'network_error' }
    }
  }

  private async loginWithPassword(login: string, password: string): Promise<CclResult<CclLoginResult>> {
    try {
      const { status, json } = await this.api('/auth/login', { body: { login, password } })
      if (status === 200 && json?.ok) return { ok: true, data: await this.applySession(json.data) }
      return this.failure(status, json)
    } catch (error) {
      console.warn('[ccl-account] loginWithPassword', error)
      return { ok: false, code: 'network_error' }
    }
  }

  private async verifyTwoFactor(challenge: string, code: string): Promise<CclResult<null>> {
    try {
      const { status, json } = await this.api('/auth/login/2fa', {
        body: { challenge_token: challenge, code },
      })
      if (status === 200 && json?.ok) {
        await this.applySession(json.data)
        return { ok: true, data: null }
      }
      return this.failure(status, json)
    } catch (error) {
      console.warn('[ccl-account] verifyTwoFactor', error)
      return { ok: false, code: 'network_error' }
    }
  }

  private async refresh(): Promise<CclResult<CclAccountState>> {
    const refreshToken = this.session?.refreshToken
    if (!refreshToken) return { ok: false, code: 'not_logged_in' }
    try {
      const { status, json } = await this.api('/auth/refresh', { body: { refresh_token: refreshToken } })
      if (status === 200 && json?.ok && json.data?.tokens?.access_token) {
        const user = this.session!.user
        this.save({
          accessToken: json.data.tokens.access_token,
          refreshToken: json.data.tokens.refresh_token ?? refreshToken,
          expiresAt: Math.floor(Date.now() / 1000) + Number(json.data.tokens.expires_in ?? 7200),
          user,
        })
        this.broadcast()
        return { ok: true, data: this.state() }
      }
      // 刷新失败说明会话已经不可救，直接清掉，避免界面一直显示「已登录」
      this.save(undefined)
      this.broadcast()
      return this.failure(status, json)
    } catch (error) {
      console.warn('[ccl-account] refresh', error)
      return { ok: false, code: 'network_error' }
    }
  }

  private async logout(): Promise<CclResult<null>> {
    const token = this.session?.accessToken
    if (token) {
      try {
        await this.api('/auth/logout', { method: 'POST', body: {}, token })
      } catch (error) {
        console.warn('[ccl-account] logout 请求失败，仍然清理本地会话', error)
      }
    }
    this.save(undefined)
    this.broadcast()
    return { ok: true, data: null }
  }

  /** 带鉴权的请求：401 时自动续期并重试一次。 */
  private async authed<T>(path: string, init: { method?: string; body?: unknown } = {}): Promise<CclResult<T>> {
    const token = await this.ensureFreshToken()
    if (!token) return { ok: false, code: 'not_logged_in' }
    try {
      let response = await this.api(path, { ...init, token })
      if (response.status === 401) {
        const refreshed = await this.refresh()
        if (!refreshed.ok) return { ok: false, code: refreshed.code }
        response = await this.api(path, { ...init, token: this.session?.accessToken })
      }
      if (response.json?.ok) return { ok: true, data: response.json.data as T }
      return this.failure(response.status, response.json)
    } catch (error) {
      console.warn(`[ccl-account] ${path}`, error)
      return { ok: false, code: 'network_error' }
    }
  }

  private updateUser(user: CclAccountUser) {
    if (!this.session) return
    this.save({ ...this.session, user })
    this.broadcast()
  }

  private async changePassword(oldPassword: string, newPassword: string): Promise<CclResult<null>> {
    const result = await this.authed<{ tokens: any; user: UserPayload }>('/me/password', {
      body: { old_password: oldPassword, new_password: newPassword },
    })
    if (!result.ok) return result
    await this.applySession(result.data)
    return { ok: true, data: null }
  }

  // ── 游戏角色 ──────────────────────────────────────────────────────────

  private async bindMinecraft(name: string, uuid: string): Promise<CclResult<CclAccountUser>> {
    const result = await this.authed<{ user: UserPayload }>('/me/minecraft/bind', { body: { name, uuid } })
    if (!result.ok) return result
    const user = toUser(result.data.user)
    this.updateUser(user)
    return { ok: true, data: user }
  }

  private async unbindMinecraft(): Promise<CclResult<CclAccountUser>> {
    const result = await this.authed<{ user: UserPayload }>('/me/minecraft/bind', { method: 'DELETE' })
    if (!result.ok) return result
    const user = toUser(result.data.user)
    this.updateUser(user)
    return { ok: true, data: user }
  }

  // ── 设置云同步 ────────────────────────────────────────────────────────

  private get settingFile() {
    return join(app.getPath('userData'), 'setting.json')
  }

  private readSettingsFile(): Record<string, unknown> {
    try {
      return JSON.parse(readFileSync(this.settingFile, 'utf8')) as Record<string, unknown>
    } catch {
      return {}
    }
  }

  private async syncUpload(): Promise<CclResult<CclSyncResult>> {
    const local = this.readSettingsFile()
    const items = SYNC_KEYS.filter((key) => key in local).map((key) => ({
      key: `${SYNC_PREFIX}${key}`,
      value: local[key],
    }))
    if (items.length === 0) return { ok: false, code: 'nothing_to_sync' }

    const result = await this.authed<{ updated: number }>('/me/settings', { method: 'PUT', body: { items } })
    if (!result.ok) return result
    return { ok: true, data: { keys: items.length, changed: [] } }
  }

  private async syncDownload(): Promise<CclResult<CclSyncResult>> {
    const result = await this.authed<{ items: { key: string; value: unknown }[] }>('/me/settings')
    if (!result.ok) return result

    const local = this.readSettingsFile()
    const patch: Record<string, unknown> = {}
    for (const item of result.data.items ?? []) {
      if (!item.key.startsWith(SYNC_PREFIX)) continue
      const key = item.key.slice(SYNC_PREFIX.length)
      if (!(SYNC_KEYS as readonly string[]).includes(key)) continue
      if (JSON.stringify(local[key]) !== JSON.stringify(item.value)) patch[key] = item.value
    }

    const changed = Object.keys(patch)
    if (changed.length === 0) return { ok: true, data: { keys: 0, changed: [] } }

    // 直接改 setting.json 会被运行中的实例在下次保存时覆盖，
    // 所以写入 pending 文件，由下次启动在读取配置前合并。
    const pending = join(app.getPath('userData'), 'ccl-settings-pending.json')
    writeFileSync(pending, JSON.stringify({ patch }, null, 2), 'utf8')
    return { ok: true, data: { keys: changed.length, changed } }
  }

  // ── 好友房间目录 ──────────────────────────────────────────────────────

  private async listRooms(): Promise<CclResult<CclRoom[]>> {
    const result = await this.authed<{ rooms: CclRoom[] }>('/rooms')
    if (!result.ok) return result
    return { ok: true, data: result.data.rooms ?? [] }
  }

  private async publishRoom(payload: CclRoomPayload): Promise<CclResult<CclRoom>> {
    const result = await this.authed<{ room: CclRoom }>('/rooms', { body: payload })
    if (!result.ok) return result
    return { ok: true, data: result.data.room }
  }

  private async heartbeatRoom(id: string, players?: number): Promise<CclResult<CclRoom>> {
    const result = await this.authed<{ room: CclRoom }>(`/rooms/${id}`, {
      method: 'PATCH',
      body: { players },
    })
    if (!result.ok) return result
    return { ok: true, data: result.data.room }
  }

  private async removeRoom(id: string): Promise<CclResult<null>> {
    const result = await this.authed<unknown>(`/rooms/${id}`, { method: 'DELETE' })
    if (!result.ok) return result
    return { ok: true, data: null }
  }

  /** 渲染层要做需要鉴权的请求时，先用这个拿一份不过期的令牌。 */
  async ensureFreshToken(): Promise<string | undefined> {
    if (!this.session) return undefined
    if (this.session.expiresAt * 1000 > Date.now() + 60_000) return this.session.accessToken
    const result = await this.refresh()
    return result.ok ? this.session?.accessToken : undefined
  }

  get user() {
    return this.session?.user
  }
}
