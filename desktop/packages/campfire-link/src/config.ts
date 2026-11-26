/**
 * EasyTier 实例配置（TOML）。
 *
 * 结构与默认值对齐 Campfire（GPL-3.0）的 `EasyTierConfig.cs`：用户态代理
 * （`no_tun`），不需要 root / TUN 设备，因此 Windows、macOS、Linux 都能直接跑。
 */
import os from 'node:os'

export interface EasyTierConfigOptions {
  networkName: string
  networkSecret: string
  /** 虚拟网络里的主机名 */
  hostname: string
  /** 固定虚拟地址；不传则用 DHCP */
  ipv4?: string
  /** 启动时连接的 peer URI */
  peers?: string[]
  /** 允许其他成员访问的本机 TCP 端口 */
  tcpWhitelist?: number[]
  /** 允许其他成员访问的本机 UDP 端口 */
  udpWhitelist?: number[]
  /** 只走直连，不经中继 */
  directOnly?: boolean
  /** 只走 P2P（打洞）连接，不用中继转发；打洞失败就完全连不上 */
  p2pOnly?: boolean
  /** 隐藏本机地址，全部走中继 */
  hideAddress?: boolean
  /** Windows 7 需要关掉 QUIC */
  legacyWindows?: boolean
}

function quote(value: string): string {
  return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`
}

/** Windows 7 及更早：EasyTier 的 QUIC 在这些系统上不可用。 */
export function isLegacyWindows(platform = process.platform, release = os.release()): boolean {
  if (platform !== 'win32') return false
  const [major = 0, minor = 0] = release.split('.').map((value) => Number.parseInt(value, 10))
  return major < 6 || (major === 6 && minor < 2)
}

function array(values: readonly string[]): string {
  return `[${values.map(quote).join(', ')}]`
}

function ports(values: readonly number[] | undefined): string[] {
  return (values ?? []).map((value) => String(value))
}

export function renderEasyTierConfig(options: EasyTierConfigOptions): string {
  const lines: string[] = []

  lines.push(`hostname = ${quote(options.hostname)}`)
  lines.push(options.ipv4 ? `ipv4 = ${quote(options.ipv4)}` : 'dhcp = true')
  lines.push(
    options.hideAddress
      ? 'listeners = []'
      : 'listeners = ["tcp://0.0.0.0:0", "udp://0.0.0.0:0"]',
  )
  lines.push(`tcp_whitelist = ${array(ports(options.tcpWhitelist))}`)
  lines.push(`udp_whitelist = ${array(ports(options.udpWhitelist))}`)
  lines.push('')

  lines.push('[network_identity]')
  lines.push(`network_name = ${quote(options.networkName)}`)
  lines.push(`network_secret = ${quote(options.networkSecret)}`)
  lines.push('')

  for (const peer of options.peers ?? []) {
    lines.push('[[peer]]')
    lines.push(`uri = ${quote(peer)}`)
    lines.push('')
  }

  lines.push('[flags]')
  // 用户态代理：不建 TUN，不需要管理员权限。
  lines.push('no_tun = true')
  lines.push('latency_first = true')
  lines.push('enable_kcp_proxy = true')
  lines.push(options.legacyWindows ? 'enable_quic_proxy = false' : 'enable_quic_proxy = true')
  if (options.legacyWindows) lines.push('disable_quic_input = true')
  // Zstandard 压缩
  lines.push('data_compress_algo = 2')
  // 不替陌生网络做中继
  lines.push('private_mode = true')
  // p2p_only：完全不走中继。打洞阶段先开它，逼 EasyTier 只走直连
  const p2pOnly = options.p2pOnly ?? (options.directOnly === true && options.hideAddress !== true)
  lines.push(`p2p_only = ${p2pOnly ? 'true' : 'false'}`)
  if (options.hideAddress) {
    lines.push('disable_p2p = true')
    lines.push('disable_udp_hole_punching = true')
  }
  lines.push('')

  return lines.join('\n')
}
