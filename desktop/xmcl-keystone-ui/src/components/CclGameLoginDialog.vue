<template>
  <v-dialog v-model="model" max-width="440" transition="fade-transition" content-class="elevation-0">
    <div class="ccl-game-dialog rounded-2xl overflow-hidden border border-white/10 bg-surface shadow-2xl">
      <div class="ccl-game-hero px-6 pt-6 pb-5 relative">
        <v-btn
          icon="close"
          variant="text"
          size="small"
          class="absolute right-2 top-2 opacity-60 hover:opacity-100"
          :aria-label="t('shared.close')"
          @click="close"
        />
        <div class="flex items-center gap-3">
          <div class="ccl-game-mark">C</div>
          <div class="min-w-0">
            <div class="text-lg font-bold leading-tight">{{ t('cclGame.title') }}</div>
            <div class="text-xs opacity-65 mt-0.5">{{ t('cclGame.description') }}</div>
          </div>
        </div>
      </div>

      <div class="px-6 pt-4 pb-6 flex flex-col gap-3">
        <v-alert v-if="!account.user.value" type="info" variant="tonal" density="compact">
          {{ t('cclGame.needAccount') }}
        </v-alert>

        <template v-else>
          <div class="flex items-center gap-2 text-body-2 px-1">
            <v-icon size="16" class="opacity-60">account_circle</v-icon>
            <span class="truncate">{{ account.user.value.nickname }}</span>
            <span v-if="account.user.value.mcName" class="ml-auto text-caption opacity-70">
              {{ account.user.value.mcName }}
            </span>
          </div>

          <v-text-field
            :model-value="account.user.value.email || account.user.value.username"
            :label="t('cclGame.account')"
            variant="outlined"
            density="comfortable"
            hide-details
            readonly
          />
          <v-text-field
            v-model="password"
            :label="t('cclGame.password')"
            :type="showPassword ? 'text' : 'password'"
            variant="outlined"
            density="comfortable"
            hide-details
            autocomplete="current-password"
            :append-inner-icon="showPassword ? 'visibility_off' : 'visibility'"
            @click:append-inner="showPassword = !showPassword"
            @keyup.enter="submit"
          />
          <v-text-field
            v-model="code"
            :label="t('cclGame.twoFactor')"
            :hint="t('cclGame.twoFactorHint')"
            persistent-hint
            maxlength="6"
            variant="outlined"
            density="comfortable"
            autocomplete="one-time-code"
            @keyup.enter="submit"
          />

          <v-alert v-if="error" type="error" variant="tonal" density="compact">
            {{ error }}
          </v-alert>

          <v-btn block size="large" rounded="xl" class="ccl-game-submit text-white font-bold" :loading="busy" @click="submit">
            <v-icon v-if="!busy" start>sports_esports</v-icon>
            {{ t('cclGame.add') }}
          </v-btn>
          <p class="text-caption opacity-60 leading-relaxed">
            {{ t('cclGame.hint') }}
          </p>
        </template>
      </div>
    </div>
  </v-dialog>
</template>

<script setup lang="ts">
import { CCL_YGGDRASIL, useCclAccount } from '@/composables/cclAccount'
import { useService } from '@/composables'
import { kUserContext } from '@/composables/user'
import { kSupportedAuthorityMetadata } from '@/composables/yggrasil'
import { injection } from '@/util/inject'
import { UserServiceKey } from '@xmcl/runtime-api'
import { ref } from 'vue'

const { t } = useI18n()
const account = useCclAccount()
const { users, select } = injection(kUserContext)
const { mutate: mutateAuthorities } = injection(kSupportedAuthorityMetadata)
const { login, addYggdrasilService, getSupportedAuthorityMetadata } = useService(UserServiceKey)

const shown = ref(false)
const model = computed({
  get: () => shown.value,
  set: (value: boolean) => { shown.value = value },
})

const password = ref('')
const code = ref('')
const showPassword = ref(false)
const busy = ref(false)
const error = ref('')

let ensured = false

async function ensureService() {
  if (ensured) return
  try {
    const all = await getSupportedAuthorityMetadata()
    if (!all.some((item) => item.authority === CCL_YGGDRASIL)) {
      await addYggdrasilService(CCL_YGGDRASIL)
      await mutateAuthorities()
    }
    ensured = true
  } catch {
    // 注册失败不阻塞登录，authlib-injector 仍会自行拉元数据
  }
}

function open() {
  password.value = ''
  code.value = ''
  error.value = ''
  shown.value = true
}

function close() {
  shown.value = false
  password.value = ''
  code.value = ''
}

async function submit() {
  const current = account.user.value
  if (!current || busy.value) return
  if (!password.value) {
    error.value = t('cclGame.requirePassword')
    return
  }
  busy.value = true
  error.value = ''
  try {
    await ensureService()
    const profile = await login({
      username: current.email || current.username,
      // Yggdrasil 协议没有独立的动态码字段，二次验证码直接追加在密码后面
      password: `${password.value}${code.value.trim()}`,
      authority: CCL_YGGDRASIL,
    })
    const found = users.value.find((item) => item.id === profile.id)
    if (found) select(found.id)
    close()
  } catch (e) {
    const message = (e as Error)?.message ?? ''
    if (/Invalid credentials|用户名或密码错误|ForbiddenOperation/i.test(message)) {
      error.value = t('cclAccount.error.invalid_credentials')
    } else if (/二次验证|two.?factor/i.test(message)) {
      error.value = t('cclGame.twoFactorRequired')
    } else {
      error.value = message || t('cclAccount.error.unknown')
    }
  } finally {
    busy.value = false
  }
}

defineExpose({ open, close })
</script>

<style scoped>
.ccl-game-hero {
  background:
    radial-gradient(120% 140% at 0% 0%, rgba(47, 180, 214, 0.22), transparent 62%),
    linear-gradient(180deg, rgba(47, 180, 214, 0.06), transparent);
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.06);
}

.ccl-game-mark {
  width: 44px;
  height: 44px;
  flex-shrink: 0;
  border-radius: 13px;
  display: grid;
  place-items: center;
  font-weight: 700;
  font-size: 18px;
  color: #2fb4d6;
  background: rgba(47, 180, 214, 0.12);
  border: 1px solid rgba(47, 180, 214, 0.32);
}

.ccl-game-submit {
  background: linear-gradient(135deg, #3fc0e0, #1d7f9c);
  box-shadow: 0 10px 24px -10px rgba(29, 127, 156, 0.75);
}
</style>
