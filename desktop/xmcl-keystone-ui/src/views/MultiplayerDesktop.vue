<template>
  <div data-testid="multiplayer-page" class="campfire-page h-full min-h-0 w-full overflow-y-auto">
    <div class="mx-auto flex w-full max-w-5xl flex-col gap-5 px-6 py-6">
      <div class="flex items-start gap-3">
        <v-avatar color="primary" size="44" variant="tonal">
          <v-icon>local_fire_department</v-icon>
        </v-avatar>
        <div class="min-w-0">
          <h1 class="text-h6 font-weight-bold">{{ t('multiplayer.campfireTitle') }}</h1>
          <p class="text-body-2 text-medium-emphasis">
            {{ t('multiplayer.campfireSubtitle') }}
          </p>
        </div>
        <v-spacer />
        <v-chip v-if="state.stage !== 'idle'" :color="stageColor" label size="small">
          {{ t(`multiplayer.campfireStage.${state.stage}`) }}
        </v-chip>
      </div>

      <v-alert v-if="!bridgeReady" density="compact" type="warning" variant="tonal">
        {{ t('multiplayer.campfireUnsupported') }}
      </v-alert>

      <div class="grid gap-4 md:grid-cols-2">
        <section class="campfire-card" :class="{ 'campfire-card--active': panel === 'embedded' }">
          <div class="flex items-center gap-2">
            <v-icon color="primary" size="22">bolt</v-icon>
            <span class="text-subtitle-1 font-weight-bold">{{ t('multiplayer.campfireEmbeddedTitle') }}</span>
          </div>
          <p class="flex-1 text-body-2 text-medium-emphasis">
            {{ t('multiplayer.campfireEmbeddedDescription') }}
          </p>
          <div class="flex flex-wrap items-center gap-2">
            <v-btn
              color="primary"
              :disabled="!bridgeReady"
              prepend-icon="group_add"
              @click="openEmbedded('host')"
            >
              {{ t('multiplayer.campfireCreateRoom') }}
            </v-btn>
            <v-btn
              variant="tonal"
              :disabled="!bridgeReady"
              prepend-icon="login"
              @click="openEmbedded('guest')"
            >
              {{ t('multiplayer.campfireJoinRoom') }}
            </v-btn>
          </div>
        </section>

        <section class="campfire-card">
          <div class="flex items-center gap-2">
            <v-icon color="primary" size="22">open_in_new</v-icon>
            <span class="text-subtitle-1 font-weight-bold">{{ t('multiplayer.campfireFullTitle') }}</span>
          </div>
          <p class="flex-1 text-body-2 text-medium-emphasis">
            {{ t('multiplayer.campfireFullDescription') }}
          </p>
          <div class="flex flex-wrap items-center gap-2">
            <v-btn
              :disabled="!bridgeReady"
              append-icon="launch"
              variant="tonal"
              @click="openFullCampfire"
            >
              {{ t('multiplayer.campfireOpenFull') }}
            </v-btn>
            <v-btn size="small" variant="text" @click="pickCampfire">
              {{ t('multiplayer.campfirePickPath') }}
            </v-btn>
          </div>
          <div v-if="campfireConfig.bundled" class="flex items-center gap-2 text-caption text-medium-emphasis">
            <v-chip color="primary" size="x-small" variant="flat">{{ t('multiplayer.campfireBundled') }}</v-chip>
            <span class="truncate">{{ campfireConfig.bundled }}</span>
          </div>
          <div v-else-if="campfireConfig.executable || campfireConfig.detected" class="truncate text-caption text-medium-emphasis">
            {{ campfireConfig.executable || campfireConfig.detected }}
          </div>
          <div v-else class="text-caption text-medium-emphasis">
            {{ t('multiplayer.campfireMissing') }}
          </div>
        </section>
      </div>

      <v-expand-transition>
        <section v-show="panel === 'embedded'" class="campfire-panel">
          <div class="flex flex-wrap items-center gap-2">
            <v-btn
              :color="role === 'host' ? 'primary' : undefined"
              :variant="role === 'host' ? 'flat' : 'tonal'"
              prepend-icon="wifi_tethering"
              size="small"
              @click="switchRole('host')"
            >
              {{ t('multiplayer.campfireHost') }}
            </v-btn>
            <v-btn
              :color="role === 'guest' ? 'primary' : undefined"
              :variant="role === 'guest' ? 'flat' : 'tonal'"
              prepend-icon="login"
              size="small"
              @click="switchRole('guest')"
            >
              {{ t('multiplayer.campfireGuest') }}
            </v-btn>
            <v-spacer />
            <v-btn
              v-if="state.stage !== 'idle'"
              color="error"
              prepend-icon="stop_circle"
              size="small"
              variant="tonal"
              @click="stop"
            >
              {{ t('multiplayer.campfireStop') }}
            </v-btn>
          </div>

          <div class="mt-4 flex flex-wrap items-end gap-3">
            <template v-if="role === 'host'">
              <v-text-field
                v-model.number="gamePort"
                class="max-w-[200px]"
                density="comfortable"
                hide-details
                :label="t('multiplayer.campfireGamePort')"
                type="number"
                variant="outlined"
              />
              <v-btn
                color="primary"
                :disabled="state.stage !== 'idle'"
                :loading="busy"
                prepend-icon="play_arrow"
                @click="startHost"
              >
                {{ t('multiplayer.campfireStartHost') }}
              </v-btn>
            </template>
            <template v-else>
              <v-text-field
                v-model="codeInput"
                class="min-w-[280px] flex-1"
                density="comfortable"
                hide-details
                :label="t('multiplayer.campfireRoomCode')"
                placeholder="U/XXXX-XXXX-XXXX-XXXX"
                variant="outlined"
                @keyup.enter="startGuest"
              />
              <v-btn
                color="primary"
                :disabled="state.stage !== 'idle' || !codeInput.trim()"
                :loading="busy"
                prepend-icon="play_arrow"
                @click="startGuest"
              >
                {{ t('multiplayer.campfireStartGuest') }}
              </v-btn>
            </template>
          </div>

          <div v-if="role === 'host'" class="mt-2 text-caption text-medium-emphasis">
            {{ t('multiplayer.campfireGamePortHint') }}
          </div>

          <v-alert v-if="error" class="mt-4" closable density="compact" type="error" variant="tonal" @click:close="error = ''">
            {{ error }}
          </v-alert>

          <div v-if="state.code || state.message || state.mode" class="mt-4 flex flex-col gap-3">
            <div v-if="state.code" class="flex flex-wrap items-center gap-2">
              <span class="text-caption text-medium-emphasis">{{ t('multiplayer.campfireRoomCode') }}</span>
              <code class="campfire-code">{{ state.code }}</code>
              <v-btn icon="content_copy" size="x-small" variant="text" @click="copyCode" />
              <v-chip v-if="copied" color="success" size="x-small" variant="tonal">
                {{ t('multiplayer.campfireCopied') }}
              </v-chip>
            </div>

            <div class="flex flex-wrap items-center gap-2 text-body-2">
              <v-progress-circular v-if="isWorking" indeterminate :size="14" :width="2" />
              <span>{{ state.message }}</span>
              <v-chip v-if="state.mode" :color="modeColor" label size="x-small" variant="tonal">
                {{ t(`multiplayer.campfireMode.${state.mode}`) }}
              </v-chip>
            </div>

            <v-alert v-if="state.localPort" density="compact" type="success" variant="tonal">
              {{ t('multiplayer.campfireReadyHint', { port: state.localPort }) }}
            </v-alert>
            <v-alert v-else-if="state.role === 'host' && state.code" density="compact" type="info" variant="tonal">
              {{ t('multiplayer.campfireHostHint', { port: state.gamePort ?? gamePort }) }}
            </v-alert>
          </div>
        </section>
      </v-expand-transition>

      <section class="campfire-panel mt-4">
        <div class="flex flex-wrap items-center gap-2">
          <v-icon size="18" class="opacity-70">travel_explore</v-icon>
          <span class="text-subtitle-2 font-bold">{{ t('multiplayer.cclRooms.title') }}</span>
          <v-spacer />
          <v-btn
            v-if="account.user.value"
            size="small"
            variant="text"
            prepend-icon="refresh"
            :loading="loadingRooms"
            @click="refreshRooms"
          >
            {{ t('multiplayer.cclRooms.refresh') }}
          </v-btn>
        </div>

        <v-alert v-if="!account.user.value" class="mt-3" density="compact" type="info" variant="tonal">
          <div class="flex flex-wrap items-center gap-2">
            <span>{{ t('multiplayer.cclRooms.needAccount') }}</span>
            <v-btn size="x-small" variant="text" color="primary" @click="goAccount">
              {{ t('multiplayer.cclRooms.signIn') }}
            </v-btn>
          </div>
        </v-alert>

        <template v-else>
          <div class="mt-3 flex flex-wrap items-center gap-2 text-caption">
            <template v-if="publishedId">
              <v-chip size="x-small" color="success" variant="tonal">{{ t('multiplayer.cclRooms.published') }}</v-chip>
              <span class="opacity-60">{{ t('multiplayer.cclRooms.publishedHint') }}</span>
              <v-btn size="x-small" variant="text" color="error" @click="unpublishRoom">
                {{ t('multiplayer.cclRooms.unpublish') }}
              </v-btn>
            </template>
            <template v-else-if="state.stage === 'ready' && state.code">
              <v-text-field
                v-model="roomName"
                density="compact"
                hide-details
                :label="t('multiplayer.cclRooms.name')"
                variant="outlined"
                class="max-w-[220px]"
              />
              <v-text-field
                v-model="roomVersion"
                density="compact"
                hide-details
                :label="t('multiplayer.cclRooms.version')"
                variant="outlined"
                class="max-w-[150px]"
              />
              <v-btn size="small" color="primary" variant="tonal" prepend-icon="campaign" :loading="publishing" @click="publishRoom">
                {{ t('multiplayer.cclRooms.publish') }}
              </v-btn>
            </template>
            <span v-else class="opacity-55">{{ t('multiplayer.cclRooms.hostFirst') }}</span>
          </div>

          <v-alert v-if="roomError" class="mt-3" closable density="compact" type="error" variant="tonal" @click:close="roomError = ''">
            {{ roomError }}
          </v-alert>

          <div v-if="rooms.length === 0" class="mt-4 text-caption opacity-50">
            {{ loadingRooms ? t('multiplayer.cclRooms.loading') : t('multiplayer.cclRooms.empty') }}
          </div>
          <div v-else class="mt-3 flex flex-col gap-2">
            <div v-for="room in rooms" :key="room.id" class="ccl-room">
              <div class="min-w-0 flex-grow">
                <div class="flex items-center gap-2">
                  <span class="font-semibold truncate">{{ room.name }}</span>
                  <v-chip v-if="room.version" size="x-small" variant="tonal">{{ room.version }}</v-chip>
                </div>
                <div class="text-caption opacity-55 truncate">
                  {{ t('multiplayer.cclRooms.meta', { owner: room.owner, players: room.players, max: room.max_players }) }}
                </div>
                <div v-if="room.motd" class="text-caption opacity-45 truncate">{{ room.motd }}</div>
              </div>
              <v-chip v-if="room.code === state.code" size="small" variant="tonal" color="success">
                {{ t('multiplayer.cclRooms.thisDevice') }}
              </v-chip>
              <v-btn
                v-else
                size="small"
                variant="tonal"
                color="primary"
                prepend-icon="login"
                :disabled="!!joiningId"
                :loading="joiningId === room.id"
                @click="joinRoom(room)"
              >
                {{ t('multiplayer.cclRooms.join') }}
              </v-btn>
            </div>
          </div>
        </template>
      </section>

      <EasyTierNodesNotice />
    </div>
  </div>
</template>

<script lang="ts" setup>
import type { CampfireConfig, LinkState } from '@ccl/campfire-link'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import EasyTierNodesNotice from '@/components/EasyTierNodesNotice.vue'
import { cclErrorKey, useCclAccount, type CclRoom } from '@/composables/cclAccount'

const { t } = useI18n()
const router = useRouter()
const account = useCclAccount()

const bridge = typeof campfire !== 'undefined' ? campfire : undefined
const bridgeReady = computed(() => !!bridge)

const panel = ref<'none' | 'embedded'>('none')
const role = ref<'host' | 'guest'>('host')
const busy = ref(false)
const error = ref('')
const copied = ref(false)
const codeInput = ref('')
const gamePort = ref(25565)
const campfireConfig = ref<CampfireConfig>({})

const state = ref<LinkState>({ stage: 'idle', role: 'host' })

// ── CCL 房间大厅 ──────────────────────────────────────────────────────
const rooms = ref<CclRoom[]>([])
const loadingRooms = ref(false)
const publishing = ref(false)
const joiningId = ref('')
const publishedId = ref('')
const roomError = ref('')
const roomName = ref('')
const roomVersion = ref('')
let heartbeat: ReturnType<typeof setInterval> | undefined

const isWorking = computed(() => ['preparing', 'starting', 'waiting'].includes(state.value.stage))
const stageColor = computed(() => {
  switch (state.value.stage) {
    case 'ready': return 'success'
    case 'error': return 'error'
    case 'waiting':
    case 'starting':
    case 'preparing': return 'info'
    default: return undefined
  }
})
const modeColor = computed(() => {
  if (state.value.mode === 'direct') return 'success'
  if (state.value.mode === 'relay') return 'warning'
  return 'info'
})

let dispose: (() => void) | undefined

onMounted(() => {
  if (account.user.value) refreshRooms()
  if (!bridge) return
  dispose = bridge.onState((next) => {
    state.value = next
  })
  bridge.getConfig().then((config) => {
    campfireConfig.value = config
  }).catch(() => undefined)
  bridge.getState().then((current) => {
    // 会话在主进程里常驻，页面重新挂载时要把状态接回来
    if (current && current.stage !== 'idle') {
      state.value = current
      if (current.role) role.value = current.role
    }
  }).catch(() => undefined)
})

onBeforeUnmount(() => {
  dispose?.()
  stopHeartbeat()
})

function openEmbedded(next: 'host' | 'guest') {
  panel.value = 'embedded'
  role.value = next
  error.value = ''
}

function switchRole(next: 'host' | 'guest') {
  role.value = next
  error.value = ''
}

async function startHost() {
  if (!bridge) return
  busy.value = true
  error.value = ''
  try {
    state.value = await bridge.startHost({ gamePort: Number(gamePort.value) || 25565 })
    if (state.value.error) error.value = state.value.error
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}

async function startGuest() {
  if (!bridge) return
  const code = codeInput.value.trim()
  if (!code) return
  busy.value = true
  error.value = ''
  try {
    state.value = await bridge.startGuest({ code })
    if (state.value.error) error.value = state.value.error
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}

async function stop() {
  if (!bridge) return
  await bridge.stop().catch(() => undefined)
  state.value = { stage: 'idle', role: role.value }
}

async function copyCode() {
  if (!state.value.code) return
  await navigator.clipboard.writeText(state.value.code)
  copied.value = true
  setTimeout(() => {
    copied.value = false
  }, 1500)
}

// ── CCL 房间大厅 ──────────────────────────────────────────────────────

function goAccount() {
  router.push('/account')
}

async function refreshRooms() {
  if (!account.user.value) return
  loadingRooms.value = true
  roomError.value = ''
  try {
    const result = await account.listRooms()
    if (result.ok) rooms.value = result.data
    else roomError.value = t(cclErrorKey(result.code))
  } finally {
    loadingRooms.value = false
  }
}

function startHeartbeat() {
  stopHeartbeat()
  heartbeat = setInterval(() => {
    const id = publishedId.value
    if (id) account.heartbeatRoom(id).catch(() => undefined)
  }, 4 * 60 * 1000)
}

function stopHeartbeat() {
  if (heartbeat) clearInterval(heartbeat)
  heartbeat = undefined
}

async function publishRoom() {
  if (!state.value.code || publishing.value) return
  publishing.value = true
  roomError.value = ''
  try {
    const result = await account.publishRoom({
      name: roomName.value.trim() || `${account.user.value?.nickname ?? 'CCL'} 的房间`,
      code: state.value.code,
      game_port: state.value.gamePort ?? (Number(gamePort.value) || 25565),
      version: roomVersion.value.trim() || undefined,
      players: 1,
    })
    if (!result.ok) {
      roomError.value = t(cclErrorKey(result.code))
      return
    }
    publishedId.value = result.data.id
    startHeartbeat()
    await refreshRooms()
  } finally {
    publishing.value = false
  }
}

async function unpublishRoom() {
  const id = publishedId.value
  if (!id) return
  publishedId.value = ''
  stopHeartbeat()
  await account.removeRoom(id).catch(() => undefined)
  await refreshRooms()
}

async function joinRoom(room: CclRoom) {
  if (!bridge || joiningId.value) return
  joiningId.value = room.id
  error.value = ''
  try {
    if (state.value.stage !== 'idle') await stop()
    role.value = 'guest'
    codeInput.value = room.code
    panel.value = 'embedded'
    await startGuest()
  } finally {
    joiningId.value = ''
  }
}

async function openFullCampfire() {
  if (!bridge) return
  error.value = ''
  try {
    await bridge.openFull()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

async function pickCampfire() {
  if (!bridge) return
  campfireConfig.value = await bridge.pickExecutable()
}
</script>

<style scoped>
.campfire-page {
  background: rgba(var(--v-theme-background), 0.6);
}

.campfire-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 18px;
  border: 1px solid rgba(var(--v-border-color), var(--v-border-opacity));
  border-radius: 14px;
  background: rgba(var(--v-theme-surface), 0.65);
  transition: border-color 0.15s ease, background-color 0.15s ease;
}

.campfire-card--active {
  border-color: rgb(var(--v-theme-primary));
}

.campfire-panel {
  padding: 18px;
  border: 1px solid rgba(var(--v-border-color), var(--v-border-opacity));
  border-radius: 14px;
  background: rgba(var(--v-theme-surface), 0.65);
}

.campfire-code {
  padding: 4px 10px;
  border-radius: 8px;
  background: rgba(var(--v-theme-primary), 0.12);
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 15px;
  letter-spacing: 0.06em;
}

.ccl-room {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid rgba(var(--v-border-color), var(--v-border-opacity));
  background: rgba(var(--v-theme-surface), 0.5);
}
</style>
