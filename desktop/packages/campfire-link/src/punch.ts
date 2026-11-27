/**
 * 「打洞优先」（P2P first）。
 *
 * 思路对齐 Minecraft Connect Tool（MIT，OpenP2P）的体验：先试直连打洞，
 * 打洞成功就直接传数据，连不上才退回中继。我们的联机栈是 EasyTier，
 * 穿透算法由它负责，这里只做三件事：
 *
 *   1. **判定**：`easytier-cli peer` 的 `cost` 列写明链路形态 ——
 *      `p2p` 是直连，`relay(n)` 是经中继（n 为跳数），`Local` 是自己。
 *   2. **等 RPC**：EasyTier 刚启动时 RPC 还没起来，CLI 调用会失败，
 *      打洞窗口不能被这些失败吃掉。
 *   3. **持续观察**：走中继连上之后 EasyTier 仍会继续尝试直连
 *      （`latency_first`），所以状态要能从中继翻回直连。
 */
import type { EasyTierInstance } from './easytier'

/**
 * - `punching`：正在打洞，还没连上房主
 * - `direct`：直连（打洞成功，数据不经中继）
 * - `relay`：经公共节点中继
 */
export type LinkMode = 'punching' | 'direct' | 'relay'

/** 打洞窗口：超过这个时间还没直连就退回中继，别让用户干等。 */
export const DEFAULT_PUNCH_TIMEOUT_MS = 15_000

/** 连上之后升级判定的轮询间隔。 */
export const LINK_POLL_INTERVAL_MS = 3_000

/** 把 `easytier-cli` 的表格输出拆成数据行（去掉表头与 `---` 分隔线）。 */
function parseTable(output: string): string[][] {
  const rows: string[][] = []
  for (const raw of output.split(/\r?\n/)) {
    const line = raw.trim()
    if (!line.startsWith('|')) continue
    const cells = line.split('|').slice(1, -1).map((cell) => cell.trim())
    if (cells.length === 0) continue
    if (cells.every((cell) => cell === '' || /^[-:]+$/.test(cell))) continue
    if (cells[0] === 'ipv4' || cells[0] === 'src') continue
    rows.push(cells)
  }
  return rows
}

/** `10.144.144.1/24` 这种带掩码的单元格也算命中。 */
function sameAddress(cell: string, ipv4: string): boolean {
  return cell === ipv4 || cell.startsWith(`${ipv4}/`)
}

function isDirectCost(cost: string): boolean {
  const value = cost.trim().toLowerCase()
  if (value.startsWith('p2p')) return true
  if (value.startsWith('relay')) return false
  // 老版本把 cost 写成跳数：1 跳即直连
  const hops = Number.parseInt(value, 10)
  return Number.isFinite(hops) && hops <= 1
}

/**
 * 房主地址是否已是直连（P2P）对等节点。
 *
 * 主判据是 `peer` 表里该行 `cost` 是否为 `p2p`。若 `peer` 表里还没有房主，
 * 再退一步看 `route` 表：直连路由的 `next_hop_ipv4` 会标 `DIRECT`，
 * 经中继时填的是中继节点地址（有时为空，只有 next_hop_hostname）。
 */
export async function isDirectPeer(network: EasyTierInstance, ipv4: string): Promise<boolean> {
  try {
    const rows = parseTable(await network.runCli(['peer']))
    const row = rows.find((cells) => sameAddress(cells[0] ?? '', ipv4))
    if (row) return isDirectCost(row[2] ?? '')
  } catch {
    // CLI 未就绪：当作未直连
  }
  try {
    const rows = parseTable(await network.runCli(['route']))
    const row = rows.find((cells) => sameAddress(cells[0] ?? '', ipv4))
    if (!row) return false
    return (row[3] ?? '').toUpperCase() === 'DIRECT'
  } catch {
    return false
  }
}

/** 等 EasyTier 本地 RPC 起来（CLI 能查到 peer 表），超时返回 false。 */
export async function waitForRpc(network: EasyTierInstance, timeoutMs = 15_000): Promise<boolean> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    if (!network.alive) return false
    try {
      await network.runCli(['peer'])
      return true
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 300))
    }
  }
  return false
}

/**
 * 持续观察链路形态，形态变化时回调；返回停止函数。
 *
 * 只在 `direct` / `relay` 之间翻转，不回调 `punching`。
 */
export function watchLinkMode(
  network: EasyTierInstance,
  ipv4: string,
  onChange: (mode: Exclude<LinkMode, 'punching'>) => void,
  intervalMs = LINK_POLL_INTERVAL_MS,
): () => void {
  let stopped = false
  let last: LinkMode | undefined
  const tick = async () => {
    if (stopped || !network.alive) return
    const direct = await isDirectPeer(network, ipv4)
    if (stopped) return
    const mode: Exclude<LinkMode, 'punching'> = direct ? 'direct' : 'relay'
    if (mode !== last) {
      last = mode
      onChange(mode)
    }
  }
  const timer = setInterval(() => {
    void tick()
  }, intervalMs)
  timer.unref?.()
  return () => {
    stopped = true
    clearInterval(timer)
  }
}
