<template>
  <section class="easytier-notice">
    <div class="easytier-notice__head">
      <v-icon color="primary" size="18">public</v-icon>
      <span class="text-body-2 font-weight-medium">{{ t('multiplayer.easyTierNotice') }}</span>
      <v-chip color="primary" label size="x-small" variant="tonal">
        {{ t('multiplayer.easyTierNodeCount', { count: nodes.length }) }}
      </v-chip>
      <v-spacer />
      <v-btn
        size="x-small"
        variant="text"
        :append-icon="expanded ? 'expand_less' : 'expand_more'"
        @click="expanded = !expanded"
      >
        {{ t('multiplayer.easyTierNodes') }}
      </v-btn>
      <v-btn
        :href="source.url"
        append-icon="open_in_new"
        size="x-small"
        target="browser"
        variant="text"
      >
        {{ t('multiplayer.easyTierLiveStatus') }}
      </v-btn>
    </div>
    <div class="text-caption text-medium-emphasis">
      {{ t('multiplayer.easyTierNoticeDescription') }}
    </div>

    <v-expand-transition>
      <div v-if="expanded" class="easytier-notice__body">
        <div v-for="node in nodes" :key="node.id" class="easytier-node">
          <div class="min-w-0 flex-1">
            <div class="truncate text-body-2">
              <span class="font-weight-medium">{{ node.provider }}</span>
              <span class="text-medium-emphasis"> · {{ node.location }}</span>
            </div>
            <div class="truncate text-caption text-medium-emphasis">{{ node.url }}</div>
          </div>
          <div class="flex flex-shrink-0 items-center gap-1">
            <v-chip label size="x-small" variant="outlined">{{ node.protocol.toUpperCase() }}</v-chip>
            <v-chip
              v-if="node.relay"
              color="success"
              label
              size="x-small"
              variant="tonal"
            >
              {{ t('multiplayer.easyTierRelay') }}
            </v-chip>
            <v-chip v-else color="warning" label size="x-small" variant="tonal">
              {{ t('multiplayer.easyTierNoRelay') }}
            </v-chip>
            <v-chip v-if="node.limited" color="warning" label size="x-small" variant="tonal">
              {{ t('multiplayer.easyTierLimited') }}
            </v-chip>
            <v-chip v-if="node.masked" color="info" label size="x-small" variant="tonal">
              {{ t('multiplayer.easyTierMasked') }}
            </v-chip>
          </div>
        </div>
        <div class="mt-2 text-caption text-medium-emphasis">
          {{ t('multiplayer.easyTierMaskedHint', { groups: source.groups.join(' / ') }) }}
        </div>
        <div class="mt-1 text-caption text-medium-emphasis">
          {{ t('multiplayer.easyTierSource', { source: source.title, date: source.updated }) }}
        </div>
      </div>
    </v-expand-transition>
  </section>
</template>

<script lang="ts" setup>
import { easyTierNodeSource as source, easyTierPublicNodes as nodes } from '@/util/easyTierNodes'

const { t } = useI18n()
const expanded = ref(false)
</script>

<style scoped>
.easytier-notice {
  padding: 10px 12px;
  border: 1px solid rgba(var(--v-theme-primary), 0.25);
  border-left: 3px solid rgb(var(--v-theme-primary));
  border-radius: 6px;
  background: rgba(var(--v-theme-primary), 0.05);
}

.easytier-notice__head {
  display: flex;
  align-items: center;
  gap: 8px;
}

.easytier-notice__body {
  margin-top: 10px;
  display: grid;
  gap: 6px;
}

.easytier-node {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 10px;
  padding: 6px 8px;
  border-radius: 4px;
  background: rgba(var(--v-theme-on-surface), 0.04);
}
</style>
