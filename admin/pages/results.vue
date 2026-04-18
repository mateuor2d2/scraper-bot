<script setup>
definePageMeta({ middleware: "auth" })
const token = useAdminToken()
const { data, pending, error } = useFetch('/api/admin/results', {
  baseURL: useRuntimeConfig().public.apiBase,
  query: { token },
  server: false
})
</script>

<template>
  <div>
    <h2 class="text-lg font-semibold mb-4">🔍 Resultados recientes</h2>
    <div v-if="pending" class="text-neutral-500">Cargando...</div>
    <div v-else-if="error" class="text-red-500">Error cargando datos</div>
    <div v-else class="space-y-3">
      <UCard v-for="r in data" :key="r.id">
        <div class="flex items-start justify-between">
          <div>
            <div class="font-medium">{{ r.title || 'Sin título' }}</div>
            <div class="text-sm text-neutral-500">{{ r.config_name }} • {{ r.scraped_at }}</div>
          </div>
          <UBadge :color="r.notified ? 'success' : 'warning'" variant="subtle">
            {{ r.notified ? 'Notificado' : 'Pendiente' }}
          </UBadge>
        </div>
        <div v-if="r.url" class="mt-2">
          <UButton :to="r.url" target="_blank" size="xs" color="neutral" variant="link">
            Ver enlace →
          </UButton>
        </div>
      </UCard>
    </div>
  </div>
</template>