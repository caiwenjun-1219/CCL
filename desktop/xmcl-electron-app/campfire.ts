/**
 * Campfire 联机的 IPC 通道名。
 *
 * 契约类型定义在 `@ccl/campfire-link`（渲染层也要用），这里只放通道常量，
 * 避免把 electron 依赖带进 UI 的类型检查。
 */
export type {
  CampfireBridge,
  CampfireConfig,
  CampfireStartGuestOptions,
  CampfireStartHostOptions,
} from '@ccl/campfire-link'

export const campfireChannels = {
  startHost: 'campfire:start-host',
  startGuest: 'campfire:start-guest',
  stop: 'campfire:stop',
  state: 'campfire:state',
  getState: 'campfire:get-state',
  openFull: 'campfire:open-full',
  pickExecutable: 'campfire:pick-executable',
  getConfig: 'campfire:get-config',
} as const
