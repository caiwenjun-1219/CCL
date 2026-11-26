import { ref } from 'vue'

/**
 * LittleSkin 皮肤站的 Yggdrasil API 地址。
 * 与 PCL 一致，登录时直接作为 authority 使用。
 */
export const LITTLESKIN_AUTHORITY = 'https://littleskin.cn/api/yggdrasil'
export const LITTLESKIN_HOME = 'https://littleskin.cn'
export const LITTLESKIN_REGISTER = 'https://littleskin.cn/auth/register'
export const LITTLESKIN_FORGOT_PASSWORD = 'https://littleskin.cn/auth/forgot'

/**
 * 内联的小图标，避免离线时依赖皮肤站 favicon。
 */
export const LITTLESKIN_ICON = 'data:image/svg+xml,' + encodeURIComponent(
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">'
  + '<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">'
  + '<stop offset="0" stop-color="#63b3ff"/><stop offset="1" stop-color="#1c6ce0"/>'
  + '</linearGradient></defs>'
  + '<rect width="64" height="64" rx="16" fill="url(#g)"/>'
  + '<text x="32" y="43" font-family="Arial, Helvetica, sans-serif" font-size="25" font-weight="700" fill="#fff" text-anchor="middle">LS</text>'
  + '</svg>',
)

const shown = ref(false)
const presetUsername = ref('')

/**
 * 全局共享的 LittleSkin 登录状态，任何组件都能打开同一个对话框。
 */
export function useLittleSkinLogin() {
  return {
    shown,
    presetUsername,
    open(username?: string) {
      presetUsername.value = username ?? ''
      shown.value = true
    },
    close() {
      shown.value = false
    },
  }
}
