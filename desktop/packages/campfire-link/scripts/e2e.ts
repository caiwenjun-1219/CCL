/**
 * 端到端验证：房主开房 + 客方加入 + 通过转发端口访问房主的「游戏服务器」。
 *
 * 同一台机器上跑两个 EasyTier 实例（host 固定 IP、guest DHCP），
 * 验证房间服务、邀请码、端口转发整条链路。
 *
 * 用法：npx tsx packages/campfire-link/scripts/e2e.ts
 */
import { connect, createServer } from 'node:net'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { GuestSession, HostSession } from '../src/session'

const MAGIC = 'CCL-E2E-OK'

async function startFakeMinecraftServer(): Promise<{ port: number; close: () => Promise<void> }> {
  const server = createServer((socket) => {
    socket.write(MAGIC)
    socket.end()
  })
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', () => resolve()))
  const address = server.address()
  const port = typeof address === 'object' && address ? address.port : 0
  return {
    port,
    close: () => new Promise<void>((resolve) => server.close(() => resolve())),
  }
}

function connectAndRead(port: number, timeoutMs = 8000): Promise<string> {
  return new Promise((resolve, reject) => {
    const socket = connect(port, '127.0.0.1')
    let data = ''
    const timer = setTimeout(() => {
      socket.destroy()
      reject(new Error('读取超时'))
    }, timeoutMs)
    socket.on('data', (chunk: Buffer) => { data += chunk.toString('utf8') })
    socket.on('end', () => {
      clearTimeout(timer)
      resolve(data)
    })
    socket.on('error', (error: Error) => {
      clearTimeout(timer)
      reject(error)
    })
  })
}

async function main() {
  const baseDir = join(tmpdir(), 'ccl-campfire-e2e')
  mkdirSync(baseDir, { recursive: true })

  const game = await startFakeMinecraftServer()
  console.log(`[e2e] 假游戏服务器端口 ${game.port}`)

  const host = new HostSession({
    baseDir,
    gamePort: game.port,
    onState: (state) => console.log(`[host] ${state.stage} ${state.message ?? ''}`),
    onLog: () => {},
  })

  const hostState = await host.start()
  if (hostState.stage !== 'ready' || !hostState.code) {
    console.error('[e2e] 房主启动失败：', hostState.error)
    await host.stop()
    await game.close()
    process.exitCode = 1
    return
  }
  console.log(`[e2e] 邀请码 ${hostState.code}`)

  const guest = new GuestSession({
    baseDir,
    code: hostState.code,
    timeoutMs: 60_000,
    onState: (state) => console.log(`[guest] ${state.stage} ${state.message ?? ''}`),
    onLog: () => {},
  })

  const guestState = await guest.start()
  if (guestState.stage !== 'ready' || !guestState.localPort) {
    console.error('[e2e] 客方加入失败：', guestState.error)
    await guest.stop()
    await host.stop()
    await game.close()
    process.exitCode = 1
    return
  }

  console.log(`[e2e] 客方本地端口 ${guestState.localPort}（链路：${guestState.mode ?? '未知'}）`)
  try {
    const payload = await connectAndRead(guestState.localPort)
    console.log(`[e2e] 收到：${payload}`)
    if (payload.includes(MAGIC)) {
      console.log('[e2e] 结果：通过')
    } else {
      console.error('[e2e] 结果：内容不符')
      process.exitCode = 1
    }
  } catch (error) {
    console.error('[e2e] 结果：连接失败', (error as Error).message)
    process.exitCode = 1
  }

  await guest.stop()
  await host.stop()
  await game.close()
}

main().catch((error) => {
  console.error(error)
  process.exitCode = 1
})
