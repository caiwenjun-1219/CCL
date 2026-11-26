<script lang="ts" setup>
import CclGameLoginDialog from '@/components/CclGameLoginDialog.vue'
import PlayerAvatar from '@/components/PlayerAvatar.vue'
import { CCL_YGGDRASIL, cclErrorKey, useCclAccount } from '@/composables/cclAccount'
import { kUserContext } from '@/composables/user'
import { injection } from '@/util/inject'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

const { t } = useI18n()
const account = useCclAccount()
const { users } = injection(kUserContext)

type Mode = 'email' | 'password'
const mode = ref<Mode>('email')
const email = ref('')
const code = ref('')
const login = ref('')
const password = ref('')
const busy = ref(false)
const sending = ref(false)
const cooldown = ref(0)
const error = ref('')
const challenge = ref('')
const info = ref('')

const gameDialog = ref<InstanceType<typeof CclGameLoginDialog>>()
const showPasswordForm = ref(false)
const oldPassword = ref('')
const newPassword = ref('')
const savingPassword = ref(false)
const passwordNotice = ref('')
const binding = ref(false)
const syncing = ref<'upload' | 'download' | ''>('')
const notice = ref('')
const noticeType = ref<'success' | 'info' | 'error'>('info')

/** 本机其他 Yggdrasil 账号（LittleSkin / Ely.by 等）里的角色，可一键绑定成 CCL 的游戏角色。 */
const bindCandidates = computed(() => {
  const result: { id: string; name: string; skin?: string; authority: string; label: string }[] = []
  for (const user of users.value) {
    if (!user.authority || user.authority === CCL_YGGDRASIL) continue
    let label = user.authority
    try {
      label = new URL(user.authority).host.replace(/^www\./, '')
    } catch {
      // 保持原样
    }
    for (const profile of Object.values(user.profiles ?? {})) {
      if (profile?.id && profile?.name) {
        result.push({ id: profile.id, name: profile.name, skin: profile.textures?.SKIN?.url, authority: user.authority, label })
      }
    }
  }
  return result
})

let timer: ReturnType<typeof setInterval> | undefined

function startCooldown(seconds: number) {
  cooldown.value = Math.max(1, seconds)
  if (timer) clearInterval(timer)
  timer = setInterval(() => {
    cooldown.value -= 1
    if (cooldown.value <= 0 && timer) {
      clearInterval(timer)
      timer = undefined
    }
  }, 1000)
}

onMounted(() => {
  if (account.user.value) account.refresh().catch(() => undefined)
})

onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
})

const emailValid = computed(() => /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/.test(email.value.trim()))

async function sendCode() {
  if (!emailValid.value || sending.value || cooldown.value > 0) return
  sending.value = true
  error.value = ''
  info.value = ''
  try {
    const result = await account.requestEmailCode(email.value.trim())
    if (result.ok) {
      startCooldown(result.data.cooldown)
      info.value = t('cclAccount.codeSent', { email: email.value.trim() })
    } else {
      error.value = t(cclErrorKey(result.code))
    }
  } finally {
    sending.value = false
  }
}

async function submit() {
  if (busy.value) return
  busy.value = true
  error.value = ''
  info.value = ''
  try {
    if (challenge.value) {
      const result = await account.verifyTwoFactor(challenge.value, code.value.trim())
      if (result.ok) {
        challenge.value = ''
        code.value = ''
      } else {
        error.value = t(cclErrorKey(result.code))
      }
      return
    }
    const result = mode.value === 'email'
      ? await account.loginWithEmailCode(email.value.trim(), code.value.trim())
      : await account.loginWithPassword(login.value.trim(), password.value)
    if (!result.ok) {
      error.value = t(cclErrorKey(result.code))
      return
    }
    password.value = ''
    code.value = ''
    if (result.data.twoFactorRequired) {
      challenge.value = result.data.challengeToken ?? ''
      info.value = t('cclAccount.twoFactorHint')
    }
  } finally {
    busy.value = false
  }
}

async function logout() {
  busy.value = true
  try {
    await account.logout()
    login.value = ''
    password.value = ''
    email.value = ''
    code.value = ''
    challenge.value = ''
  } finally {
    busy.value = false
  }
}

async function refresh() {
  busy.value = true
  error.value = ''
  try {
    const result = await account.refresh()
    if (!result.ok) error.value = t(cclErrorKey(result.code))
  } finally {
    busy.value = false
  }
}

async function savePassword() {
  if (savingPassword.value) return
  if (newPassword.value.length < 8) {
    passwordNotice.value = t('cclAccount.passwordTooShort')
    return
  }
  savingPassword.value = true
  passwordNotice.value = ''
  try {
    const result = await account.changePassword(
      account.user.value?.hasPassword ? oldPassword.value : '',
      newPassword.value,
    )
    if (result.ok) {
      passwordNotice.value = t('cclAccount.passwordSaved')
      oldPassword.value = ''
      newPassword.value = ''
      showPasswordForm.value = false
    } else {
      passwordNotice.value = t(cclErrorKey(result.code))
    }
  } finally {
    savingPassword.value = false
  }
}

function showNotice(type: 'success' | 'info' | 'error', text: string) {
  noticeType.value = type
  notice.value = text
}

async function bindProfile(profile: { id: string; name: string }) {
  if (binding.value) return
  binding.value = true
  notice.value = ''
  try {
    const result = await account.bindMinecraft(profile.name, profile.id)
    if (result.ok) {
      showNotice('success', t('cclAccount.game.bound', { name: profile.name }))
    } else {
      showNotice('error', t(cclErrorKey(result.code)))
    }
  } finally {
    binding.value = false
  }
}

async function unbindGameProfile() {
  if (binding.value) return
  binding.value = true
  notice.value = ''
  try {
    const result = await account.unbindMinecraft()
    if (result.ok) showNotice('info', t('cclAccount.game.unbound'))
    else showNotice('error', t(cclErrorKey(result.code)))
  } finally {
    binding.value = false
  }
}

async function uploadSettings() {
  if (syncing.value) return
  syncing.value = 'upload'
  notice.value = ''
  try {
    const result = await account.syncUpload()
    if (result.ok) showNotice('success', t('cclAccount.sync.uploaded', { count: result.data.keys }))
    else showNotice('error', t(cclErrorKey(result.code)))
  } finally {
    syncing.value = ''
  }
}

async function downloadSettings() {
  if (syncing.value) return
  syncing.value = 'download'
  notice.value = ''
  try {
    const result = await account.syncDownload()
    if (!result.ok) {
      showNotice('error', t(cclErrorKey(result.code)))
    } else if (result.data.changed.length === 0) {
      showNotice('info', t('cclAccount.sync.upToDate'))
    } else {
      showNotice('success', t('cclAccount.sync.downloaded', { count: result.data.changed.length }))
    }
  } finally {
    syncing.value = ''
  }
}

const canSubmit = computed(() => {
  if (busy.value) return false
  if (challenge.value) return code.value.trim().length >= 6
  if (mode.value === 'email') return emailValid.value && code.value.trim().length >= 4
  return !!login.value.trim() && password.value.length > 0
})

const initial = computed(() => (account.user.value?.nickname || account.user.value?.username || 'C').slice(0, 1).toUpperCase())
</script>

<template>
  <div class="ccl-account-page h-full w-full overflow-y-auto">
    <div class="mx-auto w-full max-w-[560px] px-6 py-8">
      <div class="flex items-center gap-3 mb-6">
        <div class="brand-mark">C</div>
        <div>
          <h1 class="text-h6 font-bold leading-tight">
            {{ t('cclAccount.title') }}
          </h1>
          <p class="text-caption text-medium-emphasis">
            {{ t('cclAccount.subtitle') }}
          </p>
        </div>
      </div>

      <v-alert v-if="!account.available" type="info" variant="tonal" density="compact" class="mb-4">
        {{ t('cclAccount.desktopOnly') }}
      </v-alert>

      <template v-else-if="account.user.value">
        <v-card class="account-card pa-6" rounded="xl">
        <div class="flex items-center gap-4">
          <div class="avatar">{{ initial }}</div>
          <div class="min-w-0">
            <div class="text-subtitle-1 font-bold truncate">
              {{ account.user.value.nickname }}
            </div>
            <div class="text-caption text-medium-emphasis truncate">
              @{{ account.user.value.username }}
            </div>
          </div>
        </div>

        <div class="mt-5 flex items-center gap-2 text-body-2">
          <v-icon size="16" class="opacity-60">mail</v-icon>
          <span class="truncate">{{ account.user.value.email }}</span>
          <v-chip v-if="account.user.value.emailVerified" size="x-small" color="success" variant="tonal">
            {{ t('cclAccount.verified') }}
          </v-chip>
        </div>

        <div class="mt-3 flex items-center gap-2 text-body-2">
          <v-icon size="16" class="opacity-60">key</v-icon>
          <span class="flex-grow">
            {{ account.user.value.hasPassword ? t('cclAccount.passwordSet') : t('cclAccount.passwordUnset') }}
          </span>
          <v-btn size="x-small" variant="text" @click="showPasswordForm = !showPasswordForm">
            {{ account.user.value.hasPassword ? t('cclAccount.changePassword') : t('cclAccount.setPassword') }}
          </v-btn>
        </div>

        <div v-if="showPasswordForm" class="mt-2 flex flex-col gap-2">
          <v-text-field
            v-if="account.user.value.hasPassword"
            v-model="oldPassword"
            :label="t('cclAccount.oldPassword')"
            type="password"
            variant="outlined"
            density="compact"
            hide-details
            autocomplete="current-password"
          />
          <v-text-field
            v-model="newPassword"
            :label="t('cclAccount.newPassword')"
            type="password"
            variant="outlined"
            density="compact"
            hide-details
            autocomplete="new-password"
          />
          <div class="flex items-center gap-2">
            <v-btn size="small" color="primary" :loading="savingPassword" @click="savePassword">
              {{ t('cclAccount.savePassword') }}
            </v-btn>
            <span v-if="passwordNotice" class="text-caption opacity-70">{{ passwordNotice }}</span>
          </div>
          <p v-if="!account.user.value.hasPassword" class="text-caption opacity-55 leading-relaxed">
            {{ t('cclAccount.passwordUnsetHint') }}
          </p>
        </div>

        <v-alert v-if="account.expired.value" type="warning" variant="tonal" density="compact" class="mt-4">
          {{ t('cclAccount.expired') }}
        </v-alert>

        <div class="mt-6 flex gap-2">
          <v-btn variant="tonal" :loading="busy" prepend-icon="refresh" @click="refresh">
            {{ t('cclAccount.refresh') }}
          </v-btn>
          <v-btn variant="text" color="error" :disabled="busy" prepend-icon="logout" @click="logout">
            {{ t('cclAccount.logout') }}
          </v-btn>
        </div>
        </v-card>

        <v-card class="account-card pa-6 mt-4" rounded="xl">
          <div class="flex items-center gap-2 mb-3">
            <v-icon size="18" class="opacity-70">sports_esports</v-icon>
            <span class="text-subtitle-2 font-bold">{{ t('cclAccount.game.title') }}</span>
          </div>

          <div v-if="account.user.value.mcName" class="flex items-center gap-3">
            <PlayerAvatar
              :src="account.user.value.skinUrl || ''"
              :dimension="42"
              class="rounded-lg overflow-hidden flex-shrink-0"
            />
            <div class="min-w-0 flex-grow">
              <div class="font-semibold truncate">{{ account.user.value.mcName }}</div>
              <div class="text-caption opacity-55 truncate">{{ account.user.value.mcUuid }}</div>
            </div>
            <v-btn size="small" variant="text" color="error" :loading="binding" @click="unbindGameProfile">
              {{ t('cclAccount.game.unbind') }}
            </v-btn>
          </div>

          <template v-else>
            <p class="text-caption opacity-70 leading-relaxed mb-3">{{ t('cclAccount.game.empty') }}</p>
            <div v-if="bindCandidates.length" class="flex flex-col gap-1.5 mb-1">
              <button
                v-for="p in bindCandidates"
                :key="p.authority + p.id"
                type="button"
                class="game-profile"
                :disabled="binding"
                @click="bindProfile(p)"
              >
                <PlayerAvatar :src="p.skin || ''" :dimension="26" class="rounded-md overflow-hidden flex-shrink-0" />
                <span class="text-sm font-medium truncate">{{ p.name }}</span>
                <span class="ml-auto text-caption opacity-50 flex-shrink-0">{{ p.label }}</span>
              </button>
            </div>
            <p v-else class="text-caption opacity-55">{{ t('cclAccount.game.noCandidate') }}</p>
          </template>

          <div class="flex gap-2 mt-4">
            <v-btn size="small" variant="tonal" prepend-icon="login" :disabled="binding" @click="gameDialog?.open()">
              {{ t('cclAccount.game.use') }}
            </v-btn>
          </div>
          <p class="text-caption opacity-45 mt-3 leading-relaxed">{{ t('cclAccount.game.useHint') }}</p>
        </v-card>

        <v-card class="account-card pa-6 mt-4" rounded="xl">
          <div class="flex items-center gap-2 mb-3">
            <v-icon size="18" class="opacity-70">cloud_sync</v-icon>
            <span class="text-subtitle-2 font-bold">{{ t('cclAccount.sync.title') }}</span>
          </div>
          <p class="text-caption opacity-70 leading-relaxed mb-4">{{ t('cclAccount.sync.hint') }}</p>
          <div class="flex gap-2">
            <v-btn
              size="small"
              variant="tonal"
              prepend-icon="cloud_upload"
              :loading="syncing === 'upload'"
              :disabled="!!syncing && syncing !== 'upload'"
              @click="uploadSettings"
            >
              {{ t('cclAccount.sync.upload') }}
            </v-btn>
            <v-btn
              size="small"
              variant="tonal"
              prepend-icon="cloud_download"
              :loading="syncing === 'download'"
              :disabled="!!syncing && syncing !== 'download'"
              @click="downloadSettings"
            >
              {{ t('cclAccount.sync.download') }}
            </v-btn>
          </div>
          <p class="text-caption opacity-45 mt-3 leading-relaxed">{{ t('cclAccount.sync.downloadHint') }}</p>
        </v-card>

        <v-alert v-if="notice" :type="noticeType" variant="tonal" density="compact" class="mt-4">
          {{ notice }}
        </v-alert>
      </template>

      <v-card v-else class="account-card pa-6" rounded="xl">
        <div class="flex gap-2 mb-5">
          <v-btn
            size="small"
            :variant="mode === 'email' ? 'flat' : 'text'"
            :color="mode === 'email' ? 'primary' : undefined"
            class="rounded-pill"
            @click="mode = 'email'; error = ''"
          >
            {{ t('cclAccount.emailMode') }}
          </v-btn>
          <v-btn
            size="small"
            :variant="mode === 'password' ? 'flat' : 'text'"
            :color="mode === 'password' ? 'primary' : undefined"
            class="rounded-pill"
            @click="mode = 'password'; error = ''"
          >
            {{ t('cclAccount.passwordMode') }}
          </v-btn>
        </div>

        <template v-if="challenge">
          <p class="text-body-2 mb-4">
            {{ t('cclAccount.twoFactorHint') }}
          </p>
          <v-text-field
            v-model="code"
            :label="t('cclAccount.code')"
            variant="outlined"
            density="comfortable"
            maxlength="6"
            autocomplete="one-time-code"
          />
        </template>

        <template v-else-if="mode === 'email'">
          <v-text-field
            v-model="email"
            :label="t('cclAccount.email')"
            variant="outlined"
            density="comfortable"
            type="email"
            autocomplete="email"
          />
          <div class="flex gap-2 items-start">
            <v-text-field
              v-model="code"
              :label="t('cclAccount.code')"
              variant="outlined"
              density="comfortable"
              maxlength="6"
              class="flex-grow"
              autocomplete="one-time-code"
            />
            <v-btn
              variant="tonal"
              height="48"
              :loading="sending"
              :disabled="!emailValid || cooldown > 0 || busy"
              @click="sendCode"
            >
              {{ cooldown > 0 ? t('cclAccount.resendIn', { seconds: cooldown }) : t('cclAccount.sendCode') }}
            </v-btn>
          </div>
        </template>

        <template v-else>
          <v-text-field
            v-model="login"
            :label="t('cclAccount.account')"
            variant="outlined"
            density="comfortable"
            autocomplete="username"
          />
          <v-text-field
            v-model="password"
            :label="t('cclAccount.password')"
            variant="outlined"
            density="comfortable"
            type="password"
            autocomplete="current-password"
          />
        </template>

        <v-alert v-if="error" type="error" variant="tonal" density="compact" class="mt-2">
          {{ error }}
        </v-alert>
        <v-alert v-else-if="info" type="info" variant="tonal" density="compact" class="mt-2">
          {{ info }}
        </v-alert>

        <v-btn
          block
          color="primary"
          size="large"
          class="mt-5 rounded-lg font-semibold"
          :loading="busy"
          :disabled="!canSubmit"
          @click="submit"
        >
          {{ challenge ? t('cclAccount.verify') : mode === 'email' ? t('cclAccount.signInOrRegister') : t('cclAccount.signIn') }}
        </v-btn>

        <p v-if="mode === 'email' && !challenge" class="text-caption text-medium-emphasis mt-4 leading-relaxed">
          {{ t('cclAccount.emailHint') }}
        </p>
      </v-card>
    </div>

    <CclGameLoginDialog ref="gameDialog" />
  </div>
</template>

<style scoped>
.ccl-account-page {
  background: radial-gradient(120% 100% at 15% 0%, rgba(56, 179, 214, 0.09), transparent 60%);
}

.brand-mark {
  width: 44px;
  height: 44px;
  border-radius: 13px;
  display: grid;
  place-items: center;
  font-weight: 700;
  font-size: 18px;
  color: rgb(var(--v-theme-primary));
  background: rgba(var(--v-theme-primary), 0.12);
  border: 1px solid rgba(var(--v-theme-primary), 0.28);
}

.avatar {
  width: 52px;
  height: 52px;
  flex-shrink: 0;
  border-radius: 16px;
  display: grid;
  place-items: center;
  font-size: 22px;
  font-weight: 700;
  color: rgb(var(--v-theme-primary));
  background: rgba(var(--v-theme-primary), 0.12);
  border: 1px solid rgba(var(--v-theme-primary), 0.24);
}

.account-card {
  background: rgba(var(--v-theme-surface), 0.72);
  border: 1px solid rgba(var(--v-theme-on-surface), 0.06);
}

.game-profile {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 7px 10px;
  border-radius: 10px;
  text-align: left;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.08);
  background: rgba(var(--v-theme-on-surface), 0.02);
  transition: background 0.2s ease, border-color 0.2s ease;
}

.game-profile:hover {
  background: rgba(var(--v-theme-primary), 0.08);
  border-color: rgba(var(--v-theme-primary), 0.35);
}

.game-profile:disabled {
  opacity: 0.5;
}
</style>
