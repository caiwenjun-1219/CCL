import { DownloadUpdateTrackerEvents, ElectronUpdateOperation, ReleaseInfo } from '@xmcl/runtime-api'
import { DownloadUpdateOptions, LauncherAppUpdater } from '@xmcl/runtime/app'
import { DownloadBaseOptions, download, getDownloadBaseOptions } from '@xmcl/file-transfer'
import { onDownloadSingle } from '@xmcl/installer'
import { AnyError } from '@xmcl/utils'
import { spawn } from 'child_process'
import { createHash } from 'crypto'
import { app, shell } from 'electron'
import { createReadStream, existsSync } from 'fs'
import { chmod, mkdir, stat, unlink } from 'fs/promises'
import { basename, join } from 'path'
import { setTimeout as sleep } from 'timers/promises'
import { CCL_ACCOUNT_BASE } from '../../account'
import type ElectronLauncherApp from '../ElectronLauncherApp'

/** `/api/v1/update/check` 返回的单个版本 */
interface CclRelease {
  version: string
  notes: string | null
  sha256: string
  size: number
  downloadUrl: string
  mandatory: boolean
  publishedAt: number
}

interface CclCheckData {
  latest: CclRelease | null
  updateAvailable: boolean
  mandatory: boolean
  minVersion: string
  message: string
}

interface CclCheckResponse {
  ok: boolean
  data?: CclCheckData
  error?: { code?: string; message?: string }
}

async function sha256File(path: string) {
  const hash = createHash('sha256')
  await new Promise<void>((resolve, reject) => {
    const stream = createReadStream(path)
    stream.on('data', (chunk) => hash.update(chunk))
    stream.on('error', reject)
    stream.on('end', () => resolve())
  })
  return hash.digest('hex')
}

/**
 * CCL 版更新器：走自家更新服务（zh.ccl.caiwenjun.top），不再查 XMCL / GitHub。
 *
 * 旧实现查 `api.xmcl.app/latest`，拿到的 operation 恒为 `manual`（我们没发
 * asar 增量包），前端于是只显示「请手动下载」的提示、连下载按钮都不给，
 * 这就是「点下载更新没反应」的原因。这里把整条链路换掉：
 *
 *   check  → 服务端给的下载地址 + sha256
 *   download → 落到 `<appData>/ccl-update/`，下完校验 sha256
 *   install → Windows 静默跑 NSIS / macOS 打开 dmg / Linux 打开文件所在目录
 */
export class CclUpdater implements LauncherAppUpdater {
  private readonly logger
  /** 本次进程内 check 到的版本，download 时按版本号取回 */
  private readonly releases = new Map<string, CclRelease>()
  /** 下载完成的安装包路径，install 时用 */
  private readonly downloaded = new Map<string, string>()

  constructor(private readonly app: ElectronLauncherApp) {
    this.logger = app.getLogger('CclUpdater')
  }

  private get platformKey() {
    const os = this.app.platform.os
    return os === 'windows' ? 'win' : os === 'osx' ? 'mac' : 'linux'
  }

  private get archKey() {
    return process.arch === 'arm64' ? 'arm64' : process.arch === 'ia32' ? 'ia32' : 'x64'
  }

  private async fetchCheck(): Promise<CclCheckData> {
    const url = new URL(`${CCL_ACCOUNT_BASE}/api/v1/update/check`)
    url.searchParams.set('platform', this.platformKey)
    url.searchParams.set('arch', this.archKey)
    url.searchParams.set('channel', 'stable')
    url.searchParams.set('version', this.app.version)

    const controller = new AbortController()
    const timer = setTimeout(() => controller.abort(), 20_000)
    const resp = await fetch(url, { signal: controller.signal }).finally(() => clearTimeout(timer))
    const text = await resp.text()
    if (!resp.ok) {
      throw new AnyError('UpdateError', `更新服务返回 ${resp.status}: ${text.slice(0, 200)}`)
    }
    const body = JSON.parse(text) as CclCheckResponse
    if (!body.ok || !body.data) {
      throw new AnyError('UpdateError', `更新服务返回异常: ${body.error?.message ?? text.slice(0, 200)}`)
    }
    return body.data
  }

  private absolute(url: string) {
    if (url.startsWith('http://') || url.startsWith('https://')) return url
    return `${CCL_ACCOUNT_BASE}/${url.replace(/^\/+/, '')}`
  }

  async checkUpdateTask(): Promise<ReleaseInfo> {
    const data = await this.fetchCheck()
    const latest = data.latest
    if (!latest) {
      // 服务端还没登记这个平台的版本：当成「已是最新」，不要让前端弹错误
      return {
        name: this.app.version,
        body: '',
        date: new Date().toISOString(),
        files: [],
        newUpdate: false,
        operation: ElectronUpdateOperation.Ccl,
      }
    }
    const url = this.absolute(latest.downloadUrl)
    this.releases.set(`v${latest.version}`, latest)
    this.logger.log(`CCL update: latest=${latest.version} current=${this.app.version} available=${data.updateAvailable} url=${url}`)
    return {
      name: `v${latest.version}`,
      body: latest.notes ?? '',
      date: new Date((latest.publishedAt || 0) * 1000).toISOString(),
      files: [{ name: basename(new URL(url).pathname) || `ccl-${latest.version}`, url }],
      newUpdate: data.updateAvailable,
      operation: ElectronUpdateOperation.Ccl,
    }
  }

  private async resolveRelease(info: ReleaseInfo): Promise<CclRelease> {
    const cached = this.releases.get(info.name)
    if (cached) return cached
    // settings 里的 updateInfo 会跨重启保留，进程内缓存没有时重新查一次
    const data = await this.fetchCheck()
    const latest = data.latest
    if (!latest || `v${latest.version}` !== info.name) {
      throw new AnyError('UpdateError', `更新服务上找不到 ${info.name}，请重新检查更新`)
    }
    this.releases.set(info.name, latest)
    return latest
  }

  async downloadUpdate(info: ReleaseInfo, options?: DownloadUpdateOptions & DownloadBaseOptions): Promise<void> {
    const release = await this.resolveRelease(info)
    const url = this.absolute(release.downloadUrl)
    const fileName = basename(new URL(url).pathname) || `ccl-${release.version}`
    const dir = join(this.app.appDataPath, 'ccl-update')
    await mkdir(dir, { recursive: true })
    const destination = join(dir, fileName)

    // 之前下过且校验通过就直接复用（重装/断网重试不会白下一遍）
    if (existsSync(destination) && release.sha256) {
      const actual = await sha256File(destination).catch(() => '')
      if (actual === release.sha256) {
        this.downloaded.set(info.name, destination)
        this.logger.log(`Reuse downloaded installer ${destination}`)
        return
      }
    }

    this.logger.log(`Download update ${url} -> ${destination}`)
    await download({
      url,
      destination,
      tracker: onDownloadSingle(options?.tracker, 'download-update.ccl', {}),
      signal: options?.abortSignal,
      expectedTotal: release.size > 0 ? release.size : undefined,
      ...getDownloadBaseOptions(options),
    })

    if (release.sha256) {
      const actual = await sha256File(destination)
      if (actual !== release.sha256) {
        await unlink(destination).catch(() => {})
        throw new AnyError('UpdateError', `更新包校验失败（期望 ${release.sha256.slice(0, 12)}…，实际 ${actual.slice(0, 12)}…），已删除`)
      }
    } else {
      // 服务端没给校验值时不装未校验的包（和安卓端一致）
      await unlink(destination).catch(() => {})
      throw new AnyError('UpdateError', '该版本没有校验值，已拒绝安装未校验的更新包')
    }

    this.downloaded.set(info.name, destination)
    const size = await stat(destination).then((s) => s.size).catch(() => 0)
    this.logger.log(`Update downloaded & verified ${destination} (${size} bytes)`)
  }

  async installUpdateAndQuit(info: ReleaseInfo): Promise<void> {
    const destination = this.downloaded.get(info.name)
      ?? join(this.app.appDataPath, 'ccl-update', basename(new URL(this.absolute((await this.resolveRelease(info)).downloadUrl)).pathname))
    if (!existsSync(destination)) {
      throw new AnyError('UpdateError', `安装包不存在：${destination}，请重新下载`)
    }

    this.logger.log(`Install update from ${destination} on ${this.platformKey}`)

    if (this.platformKey === 'win') {
      // NSIS 安装器：/S 静默安装（oneClick=false 也支持），装完自己会拉起新版本
      const child = spawn(destination, ['/S'], { detached: true, stdio: 'ignore' })
      child.unref()
      await sleep(1000)
      this.app.quit()
      return
    }

    if (this.platformKey === 'mac') {
      // 无签名 dmg：打开后由用户拖进 Applications
      await shell.openPath(destination)
      await sleep(1000)
      this.app.quit()
      return
    }

    // Linux：AppImage 直接替换正在运行的文件不安全，交给用户手动替换
    await chmod(destination, 0o755).catch(() => {})
    shell.showItemInFolder(destination)
  }
}
