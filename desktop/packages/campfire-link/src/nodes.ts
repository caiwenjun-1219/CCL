/**
 * EasyTier 公共节点（中继 / 发现）。
 *
 * 数据来自 qtet.cn 的 EasyTier 社区公共节点监控页
 * （https://info.qtet.cn/uptime/easytier），落盘在 `public-nodes.json`，
 * 启动器 UI 也直接读这份数据，避免两边各写一份。
 *
 * 双方至少要启用一个相同的节点才能互相发现；直连失败时这些节点负责转发。
 */
import data from './public-nodes.json'

export interface PublicNode {
  id: string
  /** EasyTier peer URI，如 `tcp://host:11010` / `wss://host` */
  url: string
  provider: string
  /** 出口地区 / 线路，如「上海电信」 */
  location: string
  protocol: string
  /** 是否允许作为中继转发 */
  relay: boolean
  /** 提供方限速 */
  limited?: boolean
  /** 地址被打码（含 `*`），需加入官方 QQ 群获取完整地址 */
  masked: boolean
  /** 海外节点 */
  oversea?: boolean
  note?: string
}

export interface PublicNodeSource {
  source: string
  sourceTitle: string
  updated: string
  disclaimer: string
  communityGroups: string[]
}

export const PUBLIC_NODE_SOURCE: PublicNodeSource = {
  source: data.source,
  sourceTitle: data.sourceTitle,
  updated: data.updated,
  disclaimer: data.disclaimer,
  communityGroups: data.communityGroups,
}

export const PUBLIC_NODES: PublicNode[] = data.nodes as PublicNode[]

/** 启动器里用于标注的一句话，UI 与日志共用。 */
export const PUBLIC_NODE_NOTICE = '使用 EasyTier 公共节点'

/** 默认启用的节点：延迟较低、国内可达的几个。 */
export const DEFAULT_NODE_IDS = ['sh-ct', 'xm-ct', 'cq-cm', 'zz-bgp']

export function resolveNodeUrls(ids: readonly string[], extra: readonly string[] = []): string[] {
  const known = new Map(PUBLIC_NODES.filter((node) => !node.masked).map((node) => [node.id, node.url]))
  const urls: string[] = []
  for (const id of ids) {
    const url = known.get(id)
    if (url && !urls.includes(url)) urls.push(url)
  }
  for (const url of extra) {
    if (url && !urls.includes(url)) urls.push(url)
  }
  return urls
}
