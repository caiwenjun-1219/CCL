import { AUTHORITY_DEV, AUTHORITY_MICROSOFT, AUTHORITY_MOJANG } from '@xmcl/runtime-api'

/**
 * 把账号的 authority 转成给用户看的名字。
 * 第三方（authlib-injector）账号的 authority 是一整条 API 地址，直接展示会很丑，
 * 所以常见皮肤站用品牌名，其它情况回退到域名。
 */
export function getAccountAuthorityName(authority: string, t: (key: string) => string) {
  switch (authority) {
    case AUTHORITY_MICROSOFT: return t('userServices.microsoft.name')
    case AUTHORITY_MOJANG: return t('userServices.mojang.name')
    case AUTHORITY_DEV: return t('userServices.offline.name')
  }
  try {
    const host = new URL(authority).hostname
    if (host.endsWith('littleskin.cn')) return 'LittleSkin'
    if (host.endsWith('ely.by')) return 'Ely.by'
    return host
  } catch {
    return authority
  }
}
