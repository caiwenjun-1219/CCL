<template>
  <v-dialog
    v-model="model"
    max-width="440"
    transition="fade-transition"
    content-class="elevation-0"
  >
    <div class="little-skin-dialog rounded-2xl overflow-hidden border border-white/10 bg-surface shadow-2xl select-none">
      <div class="little-skin-hero relative px-6 pt-6 pb-5">
        <v-btn
          icon="close"
          variant="text"
          size="small"
          class="absolute right-2 top-2 opacity-60 hover:opacity-100"
          :aria-label="t('shared.close')"
          @click="close"
        />
        <div class="flex items-center gap-3.5">
          <img
            v-if="isLittleSkin"
            :src="LITTLESKIN_ICON"
            class="w-12 h-12 rounded-xl shadow-lg flex-shrink-0"
            alt="LittleSkin"
          >
          <div
            v-else
            class="w-12 h-12 rounded-xl shadow-lg flex-shrink-0 flex items-center justify-center bg-primary/15 text-primary"
          >
            <v-icon size="26">dns</v-icon>
          </div>
          <div class="min-w-0">
            <div class="text-lg font-bold tracking-tight leading-tight">{{ t('littleSkin.thirdPartyTitle') }}</div>
            <div class="text-xs opacity-65 leading-snug mt-0.5">{{ t('littleSkin.thirdPartyDescription') }}</div>
          </div>
        </div>
      </div>

      <!-- 账号密码登录 -->
      <div v-if="step === 'login'" class="px-6 pt-4 pb-6 flex flex-col gap-3">
        <v-select
          v-model="serverChoice"
          :items="serverOptions"
          item-title="title"
          item-value="value"
          data-testid="littleskin-server"
          prepend-inner-icon="dns"
          variant="outlined"
          density="comfortable"
          rounded="lg"
          hide-details="auto"
          :label="t('littleSkin.server')"
        />

        <v-text-field
          v-if="serverChoice === CUSTOM_SERVER"
          v-model="customServer"
          data-testid="littleskin-custom-server"
          prepend-inner-icon="link"
          variant="outlined"
          density="comfortable"
          rounded="lg"
          hide-details="auto"
          :label="t('littleSkin.customServer')"
          :placeholder="t('littleSkin.serverPlaceholder')"
          :hint="t('littleSkin.serverHint')"
          persistent-hint
          @update:model-value="errorMessage = ''"
        />

        <v-text-field
          v-model="username"
          data-testid="littleskin-username"
          prepend-inner-icon="person"
          variant="outlined"
          density="comfortable"
          rounded="lg"
          autofocus
          hide-details="auto"
          :label="t('littleSkin.account')"
          :placeholder="t('littleSkin.accountPlaceholder')"
          @update:model-value="errorMessage = ''"
          @keypress.enter="onLogin"
        />
        <v-text-field
          v-model="password"
          data-testid="littleskin-password"
          prepend-inner-icon="lock"
          variant="outlined"
          density="comfortable"
          rounded="lg"
          hide-details="auto"
          :type="showPassword ? 'text' : 'password'"
          :label="t('littleSkin.password')"
          :placeholder="t('littleSkin.passwordPlaceholder')"
          :append-inner-icon="showPassword ? 'visibility_off' : 'visibility'"
          @click:append-inner="showPassword = !showPassword"
          @update:model-value="errorMessage = ''"
          @keypress.enter="onLogin"
        />

        <v-alert
          v-if="errorMessage"
          density="compact"
          variant="tonal"
          color="error"
          rounded="lg"
          class="text-left text-sm"
        >
          {{ errorMessage }}
        </v-alert>

        <v-checkbox
          v-model="remember"
          density="compact"
          hide-details
          color="primary"
          :label="t('littleSkin.remember')"
          class="little-skin-remember"
        />

        <v-btn
          block
          size="large"
          rounded="xl"
          class="little-skin-submit text-white font-bold tracking-wide"
          :loading="loading"
          data-testid="littleskin-submit"
          @click="onLogin"
        >
          <v-icon v-if="!loading" start>login</v-icon>
          {{ loading ? t('littleSkin.loggingIn') : t('littleSkin.login') }}
        </v-btn>

        <div v-if="serverOrigin" class="flex items-center justify-center gap-2 text-xs pt-1 flex-wrap">
          <a :href="registerUrl" target="browser" class="little-skin-link">{{ t('littleSkin.register') }}</a>
          <span class="opacity-25">·</span>
          <a :href="forgotUrl" target="browser" class="little-skin-link">{{ t('littleSkin.forgot') }}</a>
          <span class="opacity-25">·</span>
          <a :href="websiteUrl" target="browser" class="little-skin-link">{{ t('littleSkin.website') }}</a>
        </div>
      </div>

      <!-- 角色选择 -->
      <div v-else class="px-6 pt-4 pb-6 flex flex-col gap-3">
        <div>
          <div class="text-sm font-bold">{{ t('littleSkin.selectTitle') }}</div>
          <div class="text-xs opacity-60 mt-0.5">
            {{ t('littleSkin.selectDescription', { server: serverHost }) }}
          </div>
        </div>

        <div
          v-if="profiles.length === 0"
          class="rounded-xl border border-dashed py-5 px-4 text-center text-xs opacity-75"
          style="border-color: rgba(var(--v-theme-on-surface), 0.18)"
        >
          <div>{{ t('littleSkin.noCharacter') }}</div>
          <a :href="websiteUrl" target="browser" class="little-skin-link inline-block mt-2 font-medium">
            {{ t('littleSkin.createCharacter') }}
          </a>
        </div>

        <div v-else class="flex flex-col gap-1.5 max-h-[232px] overflow-y-auto pr-0.5">
          <button
            v-for="p in profiles"
            :key="p.id"
            type="button"
            class="little-skin-profile w-full flex items-center gap-3 rounded-xl px-3 py-2 text-left transition-colors"
            :class="selectedId === p.id ? 'little-skin-profile--active' : ''"
            @click="selectedId = p.id"
            @dblclick="useProfile(p.id)"
          >
            <PlayerAvatar :src="p.textures?.SKIN?.url" :dimension="34" class="overflow-hidden rounded-lg flex-shrink-0" />
            <span class="flex-grow min-w-0 truncate text-sm font-semibold">{{ p.name }}</span>
            <span v-if="p.id === currentUser?.selectedProfile" class="text-[10px] px-1.5 py-0.5 rounded opacity-60 border" style="border-color: rgba(var(--v-theme-on-surface),0.2)">
              {{ t('littleSkin.inUse') }}
            </span>
            <v-icon v-if="selectedId === p.id" size="18" color="primary">check_circle</v-icon>
          </button>
        </div>

        <v-btn
          v-if="profiles.length > 0"
          block
          size="large"
          rounded="xl"
          class="little-skin-submit text-white font-bold tracking-wide"
          :loading="selecting"
          @click="useProfile(selectedId)"
        >
          {{ t('littleSkin.useCharacter') }}
        </v-btn>
        <v-btn variant="text" size="small" block class="opacity-60" @click="backToLogin">
          <v-icon start size="16">arrow_back</v-icon>
          {{ t('shared.back') }}
        </v-btn>
      </div>
    </div>
  </v-dialog>
</template>

<script setup lang="ts">
import PlayerAvatar from '@/components/PlayerAvatar.vue'
import { useService } from '@/composables'
import {
  CUSTOM_SERVER,
  LITTLESKIN_AUTHORITY,
  LITTLESKIN_ICON,
  THIRD_PARTY_PRESETS,
  useLittleSkinLogin,
} from '@/composables/littleSkin'
import { kUserContext } from '@/composables/user'
import { kSupportedAuthorityMetadata } from '@/composables/yggrasil'
import { injection } from '@/util/inject'
import { useLocalStorage } from '@vueuse/core'
import {
  GameProfileAndTexture,
  UserException,
  UserProfile,
  UserServiceKey,
  isException,
} from '@xmcl/runtime-api'
import { computed, ref, watch } from 'vue'

const { t } = useI18n()
const { shown, presetUsername, close } = useLittleSkinLogin()
const { users, select } = injection(kUserContext)
const { mutate: mutateAuthorities } = injection(kSupportedAuthorityMetadata)
const {
  login,
  addYggdrasilService,
  getSupportedAuthorityMetadata,
  selectUserGameProfile,
} = useService(UserServiceKey)

const model = computed({
  get: () => shown.value,
  set: (v: boolean) => { shown.value = v },
})

const step = ref<'login' | 'select'>('login')
const loading = ref(false)
const selecting = ref(false)
const errorMessage = ref('')

const username = ref('')
const password = ref('')
const showPassword = ref(false)

const remember = useLocalStorage('littleSkinRemember', true)
const savedUsername = useLocalStorage('littleSkinUsername', '')
const serverChoice = useLocalStorage('littleSkinServer', LITTLESKIN_AUTHORITY as string)
const customServer = useLocalStorage('littleSkinCustomServer', '')
const serverOptions = ref<{ title: string; value: string }[]>([])

const profiles = ref<GameProfileAndTexture[]>([])
const selectedId = ref('')
const currentUser = ref<UserProfile>()

const normalizeAuthority = (url: string) => url.trim().replace(/\/+$/, '')

const authority = computed(() => {
  const raw = serverChoice.value === CUSTOM_SERVER ? customServer.value : serverChoice.value
  return normalizeAuthority(raw || '')
})

const serverHost = computed(() => {
  try {
    return new URL(authority.value).host
  } catch {
    return ''
  }
})

const serverOrigin = computed(() => {
  try {
    return new URL(authority.value).origin
  } catch {
    return ''
  }
})

const preset = computed(() => THIRD_PARTY_PRESETS.find((p) => normalizeAuthority(p.value) === authority.value))
const isLittleSkin = computed(() => preset.value?.title === 'LittleSkin'
  || /(^|\.)littleskin\.cn$/i.test(serverHost.value))
const registerUrl = computed(() => preset.value?.register || serverOrigin.value)
const forgotUrl = computed(() => preset.value?.forgot || serverOrigin.value)
const websiteUrl = computed(() => preset.value?.home || serverOrigin.value)

/**
 * 已注册的皮肤站 + 内置预设 + “自定义服务器”。
 * 与 PCL 的第三方登录服务器下拉一致。
 */
async function refreshServerOptions() {
  const list: { title: string; value: string }[] = []
  const seen = new Set<string>()
  const push = (title: string, value: string) => {
    const key = normalizeAuthority(value)
    if (!key || seen.has(key)) return
    seen.add(key)
    list.push({ title, value: key })
  }

  for (const p of THIRD_PARTY_PRESETS) push(p.title, p.value)

  try {
    const metas = await getSupportedAuthorityMetadata()
    for (const meta of metas) {
      if (meta.kind !== 'yggdrasil') continue
      try {
        const host = new URL(meta.authority).host
        push(host, meta.authority)
      } catch {
        // 忽略非法地址
      }
    }
  } catch {
    // 服务列表拉不到时至少保留内置预设
  }

  push(t('littleSkin.customServer'), CUSTOM_SERVER)
  serverOptions.value = list

  const current = normalizeAuthority(serverChoice.value)
  if (current !== CUSTOM_SERVER && !list.some((o) => o.value === current)) {
    if (current) {
      // 之前用过但当前不可用的自定义地址，保留下来让用户能继续用
      customServer.value = current
      serverChoice.value = CUSTOM_SERVER
    } else {
      serverChoice.value = LITTLESKIN_AUTHORITY
    }
  }
}

/**
 * 自定义服务器首次登录时先注册到 yggdrasil.json，
 * 这样启动时 authlib-injector 才能拿到它的元数据。
 */
async function ensureService(url: string) {
  const metas = await getSupportedAuthorityMetadata()
  const exists = metas.some((m) => m.kind === 'yggdrasil'
    && normalizeAuthority(m.authority) === url)
  if (exists) return
  await addYggdrasilService(url)
  await mutateAuthorities()
  await refreshServerOptions()
}

watch(shown, (visible) => {
  if (!visible) {
    password.value = ''
    return
  }
  step.value = 'login'
  errorMessage.value = ''
  profiles.value = []
  selectedId.value = ''
  currentUser.value = undefined
  password.value = ''
  const existing = users.value.find((u) => normalizeAuthority(u.authority) === authority.value)
  username.value = presetUsername.value || existing?.username || savedUsername.value || ''
  refreshServerOptions()
})

function toErrorMessage(e: unknown) {
  const exception = (e as { exception?: UserException['exception'] })?.exception
    ?? (isException(UserException, e) ? e.exception : undefined)
  switch (exception?.type) {
    case 'loginInvalidCredentials':
      return t('loginError.invalidCredentials')
    case 'loginInternetNotConnected':
      return t('loginError.badNetworkOrServer')
    case 'loginTimeout':
      return t('loginError.timeout')
    case 'loginReset':
      return t('loginError.connectionReset')
  }
  const message = (e as Error)?.message
  return message || t('loginError.requestFailed')
}

async function onLogin() {
  if (loading.value) return
  errorMessage.value = ''
  const url = authority.value
  if (!url) {
    errorMessage.value = t('littleSkin.requireServer')
    return
  }
  if (!/^https:\/\/[^\s]+$/i.test(url)) {
    errorMessage.value = t('littleSkin.invalidServer')
    return
  }
  if (!username.value.trim()) {
    errorMessage.value = t('littleSkin.requireAccount')
    return
  }
  if (!password.value) {
    errorMessage.value = t('littleSkin.requirePassword')
    return
  }

  loading.value = true
  try {
    // 自定义地址统一去掉尾部斜杠，避免和注册表里的写法对不上
    if (serverChoice.value === CUSTOM_SERVER) customServer.value = url
    await ensureService(url)
    const profile = await login({
      username: username.value.trim(),
      password: password.value,
      authority: url,
    })
    currentUser.value = profile
    if (remember.value) savedUsername.value = username.value.trim()

    const list = Object.values(profile.profiles ?? {})
    profiles.value = list
    selectedId.value = profile.selectedProfile || list[0]?.id || ''

    if (list.length === 0) {
      step.value = 'select'
      return
    }
    if (list.length === 1) {
      await useProfile(list[0].id)
      return
    }
    step.value = 'select'
  } catch (e) {
    errorMessage.value = toErrorMessage(e)
  } finally {
    loading.value = false
  }
}

async function useProfile(profileId: string) {
  const user = currentUser.value
  if (!user || !profileId || selecting.value) return
  selecting.value = true
  try {
    if (user.selectedProfile !== profileId) {
      await selectUserGameProfile(user, profileId)
    }
    select(user.id)
    close()
  } catch (e) {
    errorMessage.value = toErrorMessage(e)
  } finally {
    selecting.value = false
  }
}

function backToLogin() {
  step.value = 'login'
  errorMessage.value = ''
}
</script>

<style scoped>
.little-skin-hero {
  background:
    radial-gradient(120% 140% at 0% 0%, rgba(59, 130, 246, 0.22), transparent 62%),
    linear-gradient(180deg, rgba(59, 130, 246, 0.06), transparent);
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.06);
}

.little-skin-submit {
  background: linear-gradient(135deg, #4d9dff, #1d6ee0);
  box-shadow: 0 10px 24px -10px rgba(37, 116, 224, 0.75);
}

.little-skin-submit:hover {
  box-shadow: 0 14px 28px -10px rgba(37, 116, 224, 0.9);
}

.little-skin-link {
  opacity: 0.68;
  transition: opacity 0.2s ease;
}

.little-skin-link:hover {
  opacity: 1;
  text-decoration: underline;
}

.little-skin-remember :deep(.v-label) {
  font-size: 12px;
  opacity: 0.75;
}

.little-skin-profile {
  border: 1px solid rgba(var(--v-theme-on-surface), 0.08);
  background: rgba(var(--v-theme-on-surface), 0.02);
}

.little-skin-profile:hover {
  background: rgba(var(--v-theme-on-surface), 0.05);
}

.little-skin-profile--active {
  border-color: rgba(var(--v-theme-primary), 0.55);
  background: rgba(var(--v-theme-primary), 0.1);
}
</style>
