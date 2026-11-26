/**
 * 启动器与 Campfire 联机内核之间的桥接契约。
 *
 * 桌面端由主进程实现、preload 注入到渲染层；其他平台（例如 Android）可以
 * 按同一份类型实现自己的通道。
 */
import type { LinkState } from './session'

export interface CampfireConfig {
  /** 用户手动指定的 Campfire 路径，优先级最高 */
  executable?: string
  /** 随启动器一起分发的内置副本 */
  bundled?: string
  /** 系统里已安装的 Campfire，供界面展示 */
  detected?: string
}

export interface CampfireStartHostOptions {
  /** 对局域网开放的端口，默认 25565 */
  gamePort?: number
}

export interface CampfireStartGuestOptions {
  /** 房主分享的邀请码 */
  code: string
}

export interface CampfireBridge {
  startHost: (options?: CampfireStartHostOptions) => Promise<LinkState>
  startGuest: (options: CampfireStartGuestOptions) => Promise<LinkState>
  /** 当前会话状态（页面重新挂载时用来恢复） */
  getState: () => Promise<LinkState>
  stop: () => Promise<void>
  getConfig: () => Promise<CampfireConfig>
  pickExecutable: () => Promise<CampfireConfig>
  openFull: () => Promise<void>
  /** 订阅状态推送，返回取消订阅函数 */
  onState: (listener: (state: LinkState) => void) => () => void
}
