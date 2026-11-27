/**
 * EasyTier 公共节点（只读展示用）。
 *
 * 数据源与 `packages/campfire-link/src/nodes.ts` 是同一份
 * `public-nodes.json`，这里直接相对路径引入，避免 UI 与联机层各写一份。
 */
import nodeData from '../../../packages/campfire-link/src/public-nodes.json'

export interface EasyTierPublicNode {
  id: string
  provider: string
  /** 出口地区 / 线路 */
  location: string
  /** EasyTier peer URI */
  url: string
  protocol: string
  /** 是否允许中继转发 */
  relay: boolean
  /** 提供方限速 */
  limited?: boolean
  /** 地址被打码，需加官方 QQ 群拿完整地址 */
  masked: boolean
  oversea?: boolean
  note?: string
}

export const easyTierPublicNodes = nodeData.nodes as EasyTierPublicNode[]

export const easyTierNodeSource = {
  url: nodeData.source,
  title: nodeData.sourceTitle,
  updated: nodeData.updated,
  disclaimer: nodeData.disclaimer,
  groups: nodeData.communityGroups,
}
