import { injection } from '@/util/inject'
import useSWRV from 'swrv'
import { kEnvironment } from './environment'
import { kSettingsState } from './setting'
import { useSWRVConfig } from './swrvConfig'
import { CCL_ACCOUNT_BASE } from './cclAccount'

export interface LauncherNews {
  id: string
  /** 分类标签文案（公告 / 提醒 / 重要） */
  category: string
  /** 云控公告的严重级别，用于卡片配色 */
  level: 'info' | 'warn' | 'critical'
  image?: {
    url: string
    width: number
    height: number
  }
  date: string
  title: string
  description: string
  /** 「查看详情」要打开的外链，纯文本公告时为空 */
  link: string
}

interface CclAnnouncement {
  id: string
  title: string
  body: string
  level: string
  url: string
  startsAt: number
  endsAt: number
}

/**
 * 启动器公告来自 CCL 自家云控服务（云控 App 里发的公告），
 * 不再读 api.xmcl.app 的 XMCL 新闻。
 */
export function useLauncherNews() {
  const env = injection(kEnvironment)
  const { state } = injection(kSettingsState)
  const { t } = useI18n()

  const { data, error, isValidating, mutate } = useSWRV('/ccl-announcements', async () => {
    const url = new URL(`${CCL_ACCOUNT_BASE}/api/v1/announcements`)
    if (env.value) {
      url.searchParams.append('version', env.value.version)
      url.searchParams.append('os', env.value.os)
      url.searchParams.append('arch', env.value.arch)
    }
    if (state.value) {
      url.searchParams.append('locale', state.value.locale)
    }
    const resp = await fetch(url)
    if (!resp.ok) return [] as LauncherNews[]
    const result = await resp.json() as { data?: { items?: CclAnnouncement[] } }
    const items = result.data?.items ?? []
    return items.map((item): LauncherNews => ({
      id: item.id,
      category: item.level === 'critical'
        ? t('cclNotice.critical')
        : item.level === 'warn' ? t('cclNotice.warn') : t('cclNotice.info'),
      level: (item.level === 'critical' || item.level === 'warn' ? item.level : 'info'),
      date: new Date((item.startsAt || 0) * 1000).toISOString(),
      title: item.title,
      description: item.body,
      link: item.url || '',
    }))
  }, useSWRVConfig())

  const news = computed(() => data.value || [] as LauncherNews[])
  return {
    news,
    isValidating,
    mutate,
    error,
  }
}
