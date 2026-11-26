import { GuestSession, HostSession, type LinkState } from '@ccl/campfire-link'
import { type IpcMain, type WebContents, app, dialog, webContents } from 'electron'
import { spawn } from 'node:child_process'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { campfireChannels, type CampfireConfig } from '../campfire'

/**
 * Campfire 联机的桌面端控制器。
 *
 * 会话跑在主进程：EasyTier 是独立子进程，房间服务要监听端口，放到渲染层会被
 * 沙箱与 CORS 挡住。渲染层只拿状态与邀请码。
 */
export class CampfireController {
  private readonly owners = new Set<number>()
  private host?: HostSession
  private guest?: GuestSession
  private lastState: LinkState = { stage: 'idle', role: 'host' }

  constructor(private readonly ipc: IpcMain) {
    ipc.handle(campfireChannels.startHost, async (_event, options: { gamePort?: number } | undefined) => {
      await this.stopSession()
      const session = new HostSession({
        baseDir: this.baseDir,
        gamePort: options?.gamePort ?? 25565,
        onState: (state) => this.broadcast(state),
        onLog: (line) => console.log('[campfire]', line),
      })
      this.host = session
      return session.start()
    })

    ipc.handle(campfireChannels.startGuest, async (_event, options: { code: string }) => {
      await this.stopSession()
      const session = new GuestSession({
        baseDir: this.baseDir,
        code: options?.code ?? '',
        onState: (state) => this.broadcast(state),
        onLog: (line) => console.log('[campfire]', line),
      })
      this.guest = session
      return session.start()
    })

    ipc.handle(campfireChannels.stop, async () => {
      await this.stopSession()
    })

    ipc.handle(campfireChannels.getState, async () => this.lastState)

    ipc.handle(campfireChannels.getConfig, async () => this.readConfig())

    ipc.handle(campfireChannels.pickExecutable, async () => {
      const result = await dialog.showOpenDialog({
        title: '选择 Campfire 客户端',
        properties: ['openFile'],
        filters: process.platform === 'win32'
          ? [{ name: 'Campfire', extensions: ['exe'] }]
          : [{ name: 'Campfire', extensions: ['*'] }],
      })
      const picked = result.filePaths?.[0]
      if (!picked) return this.readConfig()
      this.writeConfig({ executable: picked })
      return this.readConfig()
    })

    ipc.handle(campfireChannels.openFull, async () => {
      const executable = this.resolveExecutable()
      if (!executable) throw new Error('内置的 Campfire 副本缺失，请重新安装或手动选择路径')
      const child = spawn(executable, [], { detached: true, stdio: 'ignore', cwd: dirname(executable) })
      child.unref()
    })
  }

  attach(sender: WebContents) {
    this.owners.add(sender.id)
    // 新窗口打开时补一次当前状态，避免界面停在 idle
    if (this.lastState.stage !== 'idle') sender.send(campfireChannels.state, this.lastState)
  }

  detach(senderId: number) {
    this.owners.delete(senderId)
  }

  private get baseDir() {
    return app.getPath('userData')
  }

  private get configPath() {
    return join(this.baseDir, 'campfire.json')
  }

  private readConfig(): CampfireConfig {
    let executable: string | undefined
    try {
      if (existsSync(this.configPath)) {
        const parsed = JSON.parse(readFileSync(this.configPath, 'utf8')) as CampfireConfig
        if (parsed.executable && existsSync(parsed.executable)) executable = parsed.executable
      }
    } catch {
      // 配置坏了就当没有
    }
    return { executable, bundled: this.bundledExecutable(), detected: this.detectExecutable() }
  }

  /**
   * 随安装包分发的 Campfire 副本。打包后位于 `resources/campfire`，
   * 开发模式下直接从 `build/campfire` 读取。
   */
  private bundledExecutable(): string | undefined {
    if (process.platform !== 'win32') return undefined
    const candidates = [
      join(process.resourcesPath, 'campfire', 'Campfire.exe'),
      join(app.getAppPath(), 'build', 'campfire', 'Campfire.exe'),
    ]
    return candidates.find((path) => existsSync(path))
  }

  private writeConfig(config: CampfireConfig) {
    try {
      writeFileSync(this.configPath, JSON.stringify({ executable: config.executable }, null, 2), 'utf8')
    } catch (error) {
      console.warn('[campfire] 写入配置失败', error)
    }
  }

  private detectExecutable(): string | undefined {
    const candidates: string[] = []
    if (process.platform === 'win32') {
      const local = process.env.LOCALAPPDATA
      const programFiles = process.env.ProgramFiles
      const programFilesX86 = process.env['ProgramFiles(x86)']
      if (local) {
        candidates.push(join(local, 'Programs', 'Campfire', 'Campfire.exe'))
        candidates.push(join(local, 'Campfire', 'Campfire.exe'))
      }
      if (programFiles) candidates.push(join(programFiles, 'Campfire', 'Campfire.exe'))
      if (programFilesX86) candidates.push(join(programFilesX86, 'Campfire', 'Campfire.exe'))
    } else if (process.platform === 'darwin') {
      candidates.push('/Applications/Campfire.app')
    } else {
      candidates.push('/usr/bin/campfire', '/opt/campfire/campfire')
    }
    return candidates.find((path) => existsSync(path))
  }

  private resolveExecutable(): string | undefined {
    const config = this.readConfig()
    // 用户手选 > 内置副本 > 系统已安装
    return config.executable ?? config.bundled ?? config.detected
  }

  private broadcast(state: LinkState) {
    this.lastState = state
    for (const id of this.owners) {
      const contents = webContents.fromId(id)
      if (contents && !contents.isDestroyed()) contents.send(campfireChannels.state, state)
    }
  }

  private async stopSession() {
    const host = this.host
    const guest = this.guest
    this.host = undefined
    this.guest = undefined
    await host?.stop().catch(() => undefined)
    await guest?.stop().catch(() => undefined)
  }
}
