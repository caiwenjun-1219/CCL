declare module '*.webp' {
  const value: string
  export default value
}

declare module 'grid-layout-plus' {
  export const GridLayout: import('vue').Component<any, any, any, { size: number; remain: number }>
  export const GridItem: import('vue').Component<any, any, any, { w: number }>
}

declare module '@/assets/locales/index.json' {
  type Locale = {
    [range: string]: string
  }
  const locale: Locale
  export = locale
}

declare namespace JSX {
  interface IntrinsicAttributes {
    class?: any
    style?: any
  }
}

declare module '*.png' {
  const value: string
  export default value
}
declare module '*.svg' {
  const value: string
  export default value
}

interface File {
  /**
   * The real path to the file on the users filesystem
   */
  path: string
}

declare const multiplayerNetworkDiagnostics: import('@xmcl/runtime-api').NetworkDiagnostics

/**
 * Campfire 联机桥（仅桌面端 preload 注入，Web 端为 undefined）。
 */
declare const campfire: import('@ccl/campfire-link').CampfireBridge | undefined

/**
 * CCL 账号桥（仅桌面端 preload 注入，Web 端为 undefined）。
 */
declare const cclAccount: import('@/composables/cclAccount').CclAccountBridge | undefined
