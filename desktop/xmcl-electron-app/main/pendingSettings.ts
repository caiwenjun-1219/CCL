import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

/**
 * 合并「从云端恢复设置」留下的补丁。
 *
 * AccountController 在下载设置时只写 ccl-settings-pending.json，
 * 因为运行中的启动器会在下次保存设置时用内存值覆盖 setting.json。
 * 这里在 LauncherApp 读取配置之前完成合并，保证恢复结果真正生效。
 */
export function applyPendingSettings(userData: string) {
  const pendingPath = join(userData, 'ccl-settings-pending.json')
  if (!existsSync(pendingPath)) return
  try {
    const raw = JSON.parse(readFileSync(pendingPath, 'utf8')) as { patch?: Record<string, unknown> }
    const patch = raw?.patch
    if (patch && typeof patch === 'object') {
      const settingPath = join(userData, 'setting.json')
      let current: Record<string, unknown> = {}
      try {
        current = JSON.parse(readFileSync(settingPath, 'utf8')) as Record<string, unknown>
      } catch {
        // 没有旧配置就当作全新
      }
      writeFileSync(settingPath, JSON.stringify({ ...current, ...patch }, null, 2), 'utf8')
    }
    rmSync(pendingPath, { force: true })
  } catch (error) {
    console.warn('[ccl-account] 合并待应用设置失败', error)
  }
}
