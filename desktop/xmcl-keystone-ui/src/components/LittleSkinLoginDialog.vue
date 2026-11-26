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
          <img :src="LITTLESKIN_ICON" class="w-12 h-12 rounded-xl shadow-lg flex-shrink-0" alt="LittleSkin">
          <div class="min-w-0">
            <div class="text-lg font-bold tracking-tight leading-tight">{{ t('littleSkin.name') }}</div>
            <div class="text-xs opacity-65 leading-snug mt-0.5">{{ t('littleSkin.description') }}</div>
          </div>
        </div>
      </div>

      <!-- 账号密码登录 -->
      <div v-if="step === 'login'" class="px-6 pt-4 pb-6 flex flex-col gap-3">
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

        <div class="flex items-center justify-center gap-2 text-xs pt-1 flex-wrap">
          <a :href="LITTLESKIN_REGISTER" target="browser" class="little-skin-link">{{ t('littleSkin.register') }}</a>
          <span class="opacity-25">·</span>
          <a :href="LITTLESKIN_FORGOT_PASSWORD" target="browser" class="little-skin-link">{{ t('littleSkin.forgot') }}</a>
          <span class="opacity-25">·</span>
          <a :href="LITTLESKIN_HOME" target="browser" class="little-skin-link">{{ t('littleSkin.website') }}</a>
        </div>
      </div>

      <!-- 角色选择 -->
      <div v-else class="px-6 pt-4 pb-6 flex flex-col gap-3">
        <div>
          <div class="text-sm font-bold">{{ t('littleSkin.selectTitle') }}</div>
          <div class="text-xs opacity-60 mt-0.5">{{ t('littleSkin.selectDescription') }}</div>
        </div>

        <div
          v-if="profiles.length === 0"
          class="rounded-xl border border-dashed py-5 px-4 text-center text-xs opacity-75"
          style="border-color: rgba(var(--v-theme-on-surface), 0.18)"
        >
          <div>{{ t('littleSkin.noCharacter') }}</div>
          <a :href="LITTLESKIN_HOME" target="browser" class="little-skin-link inline-block mt-2 font-medium">
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
  LITTLESKIN_AUTHORITY,
  LITTLESKIN_FORGOT_PASSWORD,
  LITTLESKIN_HOME,
  LITTLESKIN_ICON,
  LITTLESKIN_REGISTER,
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

const profiles = ref<GameProfileAndTexture[]>([])
const selectedId = ref('')
const currentUser = ref<UserProfile>()

let ensuredService = false

/**
 * 旧版本留下的 yggdrasil.json 里可能没有 LittleSkin。
 * 补注册一次，保证启动时能拿到 authlib-injector 的预置元数据。
 */
async function ensureLittleSkinService() {
  if (ensuredService) return
  try {
    const all = await getSupportedAuthorityMetadata()
    const exists = all.some((a) => {
      try {
        return new URL(a.authority).host === 'littleskin.cn'
      } catch {
        return false
      }
    })
    if (!exists) {
      await addYggdrasilService(LITTLESKIN_AUTHORITY)
      await mutateAuthorities()
    }
    ensuredService = true
  } catch {
    // 注册失败不阻塞登录，authlib-injector 仍会自行拉取元数据
  }
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
  const existing = users.value.find((u) => u.authority === LITTLESKIN_AUTHORITY)
  username.value = presetUsername.value || existing?.username || savedUsername.value || ''
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
    await ensureLittleSkinService()
    const profile = await login({
      username: username.value.trim(),
      password: password.value,
      authority: LITTLESKIN_AUTHORITY,
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
