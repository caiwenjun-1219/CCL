/**
 * 冒烟测试：下载 EasyTier → 以房主配置启动 → 观察是否连上公共节点。
 *
 * 用法：npx tsx packages/campfire-link/scripts/smoke.ts [秒数]
 */
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { isLegacyWindows, renderEasyTierConfig } from '../src/config'
import { ensureEasyTier, findFreePort, startEasyTier } from '../src/easytier'
import { DEFAULT_NODE_IDS, resolveNodeUrls } from '../src/nodes'
import { createRoomCode } from '../src/roomCode'
import { HOST_NAME_PREFIX, HOST_VIRTUAL_ADDRESS } from '../src/protocol'

async function main() {
  const seconds = Number(process.argv[2] ?? 25)
  const baseDir = join(tmpdir(), 'ccl-campfire-smoke')
  mkdirSync(baseDir, { recursive: true })

  const install = await ensureEasyTier(baseDir, (message) => console.log(`[setup] ${message}`))
  console.log(`[setup] core=${install.corePath}`)

  const info = createRoomCode()
  const configPath = join(baseDir, 'smoke-host.toml')
  writeFileSync(
    configPath,
    renderEasyTierConfig({
      networkName: info.networkName,
      networkSecret: info.networkSecret,
      hostname: `${HOST_NAME_PREFIX}13448`,
      ipv4: HOST_VIRTUAL_ADDRESS,
      peers: resolveNodeUrls(DEFAULT_NODE_IDS),
      tcpWhitelist: [13448, 25565],
      legacyWindows: isLegacyWindows(),
    }),
    'utf8',
  )
  console.log(`[setup] code=${info.code}`)

  const rpcPort = await findFreePort()
  console.log(`[setup] rpc=${rpcPort}`)

  const instance = startEasyTier({
    install,
    configPath,
    rpcPort,
    onLog: (line) => console.log(`[et] ${line}`),
  })

  await new Promise((resolve) => setTimeout(resolve, seconds * 1000))

  console.log('[check] 尝试读取 peer 列表…')
  try {
    const output = await instance.runCli(['peer'])
    console.log(output.slice(0, 2000))
  } catch (error) {
    console.log(`[check] peer 查询失败：${(error as Error).message}`)
  }

  console.log('[check] 实例存活：', instance.alive)
  await instance.stop()
  console.log('[done]')
}

main().catch((error) => {
  console.error(error)
  process.exitCode = 1
})
