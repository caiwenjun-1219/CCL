/**
 * EasyTier 二进制的获取、进程生命周期与 CLI 调用。
 *
 * Campfire（GPL-3.0）随包下载的是 `easytier-core` + `easytier-cli`（v2.6.4）。
 * 这里做同样的事，但按平台挑对应发布包，因此 Windows / macOS / Linux 都能用。
 *
 * EasyTier 本身是 LGPL-3.0：本模块以**独立进程**方式调用它，不链接其代码，
 * 分发其二进制时需同时提供 LGPL-3.0 文本与源码获取方式。
 */
import { spawn, type ChildProcess } from 'node:child_process'
import { chmodSync, createWriteStream, existsSync, mkdirSync, readdirSync, rmSync, statSync } from 'node:fs'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pipeline } from 'node:stream/promises'
import { Readable } from 'node:stream'
import { open, readAllEntries, readEntryBuffered } from '@xmcl/unzip'

export const EASYTIER_VERSION = '2.6.4'

const DOWNLOAD_HOSTS = [
  (asset: string) => `https://gitee.com/easytier/EasyTier/releases/download/v${EASYTIER_VERSION}/${asset}`,
  (asset: string) => `https://ghfast.top/https://github.com/EasyTier/EasyTier/releases/download/v${EASYTIER_VERSION}/${asset}`,
  (asset: string) => `https://gh-proxy.com/https://github.com/EasyTier/EasyTier/releases/download/v${EASYTIER_VERSION}/${asset}`,
  (asset: string) => `https://github.com/EasyTier/EasyTier/releases/download/v${EASYTIER_VERSION}/${asset}`,
]

export interface PlatformAsset {
  asset: string
  coreName: string
  cliName: string
  folder: string
}

/** 当前平台对应的 EasyTier 发布包与可执行文件名。 */
export function resolvePlatform(platform = process.platform, arch = process.arch): PlatformAsset {
  const exe = platform === 'win32' ? '.exe' : ''
  const coreName = `easytier-core${exe}`
  const cliName = `easytier-cli${exe}`

  let slug: string
  if (platform === 'win32' && arch === 'x64') slug = 'windows-x86_64'
  else if (platform === 'win32' && arch === 'arm64') slug = 'windows-arm64'
  else if (platform === 'darwin' && arch === 'arm64') slug = 'macos-aarch64'
  else if (platform === 'darwin' && arch === 'x64') slug = 'macos-x86_64'
  else if (platform === 'linux' && arch === 'x64') slug = 'linux-x86_64'
  else if (platform === 'linux' && arch === 'arm64') slug = 'linux-aarch64'
  else throw new Error(`EasyTier 没有适配当前平台：${platform}/${arch}`)

  return {
    asset: `easytier-${slug}-v${EASYTIER_VERSION}.zip`,
    coreName,
    cliName,
    folder: `${EASYTIER_VERSION}-${platform}-${arch}`,
  }
}

export interface EasyTierInstall {
  dir: string
  corePath: string
  cliPath: string
}

export function resolveInstall(baseDir: string, platform = resolvePlatform()): EasyTierInstall {
  const dir = join(baseDir, platform.folder)
  return {
    dir,
    corePath: join(dir, platform.coreName),
    cliPath: join(dir, platform.cliName),
  }
}

export function isInstalled(install: EasyTierInstall): boolean {
  return existsSync(install.corePath) && existsSync(install.cliPath)
}

async function downloadZip(asset: string, target: string): Promise<void> {
  const errors: string[] = []
  for (const build of DOWNLOAD_HOSTS) {
    const url = build(asset)
    try {
      const response = await fetch(url, { redirect: 'follow' })
      if (!response.ok || !response.body) {
        errors.push(`${url} -> HTTP ${response.status}`)
        continue
      }
      await pipeline(Readable.fromWeb(response.body as any), createWriteStream(target))
      return
    } catch (error) {
      errors.push(`${url} -> ${(error as Error).message}`)
    }
  }
  throw new Error(`下载 EasyTier 失败：\n${errors.join('\n')}`)
}

/**
 * 确保 EasyTier 已就绪：缺失时下载并解压到 `baseDir`。
 * 返回可执行文件路径。
 */
export async function ensureEasyTier(
  baseDir: string,
  onProgress?: (message: string) => void,
): Promise<EasyTierInstall> {
  const platform = resolvePlatform()
  const install = resolveInstall(baseDir, platform)
  if (isInstalled(install)) return install

  mkdirSync(install.dir, { recursive: true })
  const zipPath = join(tmpdir(), `ccl-${platform.asset}`)

  onProgress?.(`正在下载 EasyTier ${EASYTIER_VERSION}…`)
  try {
    await downloadZip(platform.asset, zipPath)
    onProgress?.('正在解压…')

    const zip = await open(zipPath)
    const entries = await readAllEntries(zip)
    const { writeFileSync } = await import('node:fs')

    // 整包解压：核心 exe 在加载期就依赖同目录的 Packet.dll 等运行库，
    // 只挑 core/cli 会让进程以 0xC0000135（STATUS_DLL_NOT_FOUND）直接退出。
    let extracted = 0
    for (const entry of entries) {
      if (entry.fileName.endsWith('/')) continue
      const name = entry.fileName.split('/').pop()
      if (!name) continue
      const data = await readEntryBuffered(zip, entry)
      const target = join(install.dir, name)
      writeFileSync(target, data)
      if (process.platform !== 'win32' && !name.endsWith('.dll') && !name.endsWith('.sys')) {
        chmodSync(target, 0o755)
      }
      extracted++
    }
    if (extracted === 0) {
      throw new Error('EasyTier 压缩包是空的')
    }
  } finally {
    try {
      rmSync(zipPath, { force: true })
    } catch {
      // 忽略清理失败
    }
  }

  if (!isInstalled(install)) {
    throw new Error(`EasyTier 解压不完整：${install.dir}`)
  }
  onProgress?.('EasyTier 就绪')
  return install
}

/** 找一个空闲的本地 TCP 端口。 */
export function findFreePort(preferred?: number): Promise<number> {
  return new Promise((resolve, reject) => {
    const tryBind = (port: number, retry: boolean) => {
      const server = createServer()
      server.unref()
      server.once('error', (error: any) => {
        if (retry && error?.code === 'EADDRINUSE') {
          tryBind(0, false)
        } else {
          reject(error)
        }
      })
      server.listen(port, '127.0.0.1', () => {
        const address = server.address()
        const actual = typeof address === 'object' && address ? address.port : port
        server.close(() => resolve(actual))
      })
    }
    tryBind(preferred ?? 0, preferred !== undefined)
  })
}

export interface EasyTierInstance {
  readonly rpcPort: number
  readonly alive: boolean
  readonly output: string
  stop(): Promise<void>
  runCli(args: string[]): Promise<string>
  addPortForward(protocol: 'tcp' | 'udp', bind: string, target: string): Promise<void>
  setUdpWhitelist(ports: number[]): Promise<void>
}

interface StartOptions {
  install: EasyTierInstall
  configPath: string
  rpcPort: number
  onLog?: (line: string) => void
}

/** 启动一个 EasyTier 实例（`-c config.toml -r 127.0.0.1:rpc --multi-thread`）。 */
export function startEasyTier(options: StartOptions): EasyTierInstance {
  const { install, configPath, rpcPort, onLog } = options
  const lines: string[] = []

  const child: ChildProcess = spawn(
    install.corePath,
    ['-c', configPath, '-r', `127.0.0.1:${rpcPort}`, '--multi-thread'],
    { cwd: install.dir, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] },
  )

  const collect = (chunk: Buffer) => {
    const text = chunk.toString('utf8')
    for (const line of text.split(/\r?\n/)) {
      if (!line.trim()) continue
      lines.push(line)
      if (lines.length > 400) lines.shift()
      onLog?.(line)
    }
  }
  child.stdout?.on('data', collect)
  child.stderr?.on('data', collect)

  let alive = true
  child.once('exit', () => {
    alive = false
  })

  const runCli = (args: string[]): Promise<string> => {
    return new Promise((resolve, reject) => {
      const cli = spawn(install.cliPath, ['-p', `127.0.0.1:${rpcPort}`, ...args], {
        cwd: install.dir,
        windowsHide: true,
        stdio: ['ignore', 'pipe', 'pipe'],
      })
      let stdout = ''
      let stderr = ''
      const timer = setTimeout(() => {
        cli.kill()
        reject(new Error(`easytier-cli ${args.join(' ')} 超时`))
      }, 10_000)
      cli.stdout?.on('data', (chunk) => (stdout += chunk.toString('utf8')))
      cli.stderr?.on('data', (chunk) => (stderr += chunk.toString('utf8')))
      cli.once('error', (error) => {
        clearTimeout(timer)
        reject(error)
      })
      cli.once('exit', (code) => {
        clearTimeout(timer)
        if (code === 0) resolve(stdout)
        else reject(new Error(`easytier-cli 退出码 ${code}: ${stderr || stdout}`))
      })
    })
  }

  return {
    rpcPort,
    get alive() {
      return alive
    },
    get output() {
      return lines.join('\n')
    },
    async stop() {
      if (!alive) return
      alive = false
      try {
        child.kill()
      } catch {
        // 已退出
      }
      await new Promise<void>((resolve) => {
        const timer = setTimeout(() => {
          try {
            child.kill('SIGKILL')
          } catch {
            // 忽略
          }
          resolve()
        }, 3000)
        child.once('exit', () => {
          clearTimeout(timer)
          resolve()
        })
      })
    },
    runCli,
    async addPortForward(protocol, bind, target) {
      let lastError: unknown
      // 刚启动时 RPC 可能还没就绪，重试 3 次（与 Campfire 一致）
      for (let attempt = 0; attempt < 3; attempt++) {
        try {
          await runCli(['port-forward', 'add', protocol, bind, target])
          return
        } catch (error) {
          lastError = error
          await new Promise((resolve) => setTimeout(resolve, 500 + attempt * 1000))
        }
      }
      throw lastError instanceof Error ? lastError : new Error('端口转发失败')
    },
    async setUdpWhitelist(ports) {
      const list = ports.length > 0 ? ports.join(',') : '0'
      await runCli(['whitelist', 'set-udp', list])
    },
  }
}

/** 读取安装目录里实际存在的文件，便于排障。 */
export function listInstallDir(dir: string): string[] {
  try {
    return readdirSync(dir).filter((name) => {
      try {
        return statSync(join(dir, name)).isFile()
      } catch {
        return false
      }
    })
  } catch {
    return []
  }
}
