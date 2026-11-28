<template>
  <v-dialog
    v-model="isShown"
    width="520"
    transition="fade-transition"
    content-class="elevation-0"
    :persistent="false"
  >
    <div v-if="current" class="flex w-full max-h-[80vh] flex-col overflow-hidden">
      <!-- Header -->
      <div class="flex items-start gap-3 px-6 pt-6 pb-3">
        <div
          class="w-10 h-10 rounded-xl flex items-center justify-center flex-shrink-0"
          :style="{ backgroundColor: `rgba(var(--v-theme-${current.themeColor}), 0.14)` }"
        >
          <v-icon size="22" :color="current.themeColor">{{ current.icon }}</v-icon>
        </div>
        <div class="flex-grow min-w-0">
          <div class="text-xs font-medium uppercase tracking-wide opacity-55">{{ current.category }}</div>
          <div class="text-lg font-bold leading-snug mt-0.5" style="color: rgba(var(--v-theme-on-surface), 0.92)">
            {{ current.title }}
          </div>
        </div>
        <v-btn icon="close" variant="text" size="small" @click="close" />
      </div>

      <!-- Body -->
      <div class="flex-1 min-h-0 overflow-y-auto invisible-scroll px-6 pb-2">
        <p
          v-if="current.description"
          class="whitespace-pre-wrap text-sm leading-relaxed m-0"
          style="color: rgba(var(--v-theme-on-surface), 0.78)"
        >{{ current.description }}</p>
        <p v-else class="text-sm m-0 opacity-55">{{ t('cclNotice.emptyBody') }}</p>
      </div>

      <!-- Actions -->
      <div class="flex items-center gap-2 px-6 pb-5 pt-4">
        <v-btn v-if="current.link" variant="text" color="primary" size="small" @click="openLink">
          {{ t('news.readMore') }}
        </v-btn>
        <v-spacer />
        <template v-if="queue.length > 1">
          <v-btn variant="text" size="small" :disabled="index === 0" @click="index--">
            {{ t('cclNotice.prev') }}
          </v-btn>
          <span class="text-xs opacity-55">{{ index + 1 }} / {{ queue.length }}</span>
          <v-btn variant="text" size="small" :disabled="index >= queue.length - 1" @click="index++">
            {{ t('cclNotice.next') }}
          </v-btn>
        </template>
        <v-btn color="primary" variant="flat" size="small" @click="close">
          {{ t('cclNotice.gotIt') }}
        </v-btn>
      </div>
    </div>
  </v-dialog>
</template>

<script lang="ts" setup>
import { LauncherNews, useLauncherNews } from '@/composables/launcherNews'

const { news } = useLauncherNews()
const { t } = useI18n()

const isShown = ref(false)
const index = ref(0)
const queue = ref<LauncherNews[]>([])

let shownThisSession = false

const current = computed(() => {
  const item = queue.value[index.value]
  if (!item) return null
  return {
    ...item,
    icon: item.level === 'critical' ? 'error' : item.level === 'warn' ? 'warning' : 'campaign',
    themeColor: item.level === 'critical' ? 'error' : item.level === 'warn' ? 'warning' : 'primary',
  }
})

// 每次启动弹一轮：公告是运营动作，一直挂着的时候就该每次提醒，
// 而不是「点过一次就永远不再出现」。
watch(news, (list) => {
  if (shownThisSession || isShown.value || !list.length) return
  queue.value = list
  index.value = 0
  isShown.value = true
  shownThisSession = true
}, { immediate: true })

function close() {
  isShown.value = false
}

function openLink() {
  const link = current.value?.link
  if (!link) return
  window.open(link, '_blank', 'noopener,noreferrer')
}
</script>
