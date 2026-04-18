<script setup>
definePageMeta({ middleware: "auth" })
const token = useAdminToken()
const { data, pending, error } = useFetch('/api/admin/dashboard', {
  baseURL: useRuntimeConfig().public.apiBase,
  query: { token },
  server: false
})

const scrapeColumns = [
  { accessorKey: 'config_name', header: 'Búsqueda' },
  { accessorKey: 'status', header: 'Estado' },
  { accessorKey: 'items_found', header: 'Items' },
  { accessorKey: 'duration_ms', header: 'Duración (ms)' },
  { accessorKey: 'created_at', header: 'Fecha' },
]
</script>

<template>
  <div>
    <h2 class="text-lg font-semibold mb-4">Dashboard</h2>
    <div v-if="pending" class="text-neutral-500">Cargando...</div>
    <div v-else-if="error" class="text-red-500">Error cargando datos</div>
    <div v-else class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
      <UCard>
        <template #header>
          <div class="text-sm text-neutral-500">Usuarios totales</div>
        </template>
        <div class="text-3xl font-bold">{{ data?.total_users || 0 }}</div>
      </UCard>
      <UCard>
        <template #header>
          <div class="text-sm text-neutral-500">Búsquedas activas</div>
        </template>
        <div class="text-3xl font-bold">{{ data?.total_searches || 0 }}</div>
      </UCard>
      <UCard>
        <template #header>
          <div class="text-sm text-neutral-500">Resultados hoy</div>
        </template>
        <div class="text-3xl font-bold">{{ data?.total_results_today || 0 }}</div>
      </UCard>
      <UCard>
        <template #header>
          <div class="text-sm text-neutral-500">Suscripciones activas</div>
        </template>
        <div class="text-3xl font-bold">{{ data?.active_subscriptions || 0 }}</div>
      </UCard>

      <UCard class="md:col-span-2 lg:col-span-4">
        <template #header>
          <h3 class="font-semibold">📊 Últimos scrapes</h3>
        </template>
        <UTable :data="data?.recent_scrapes || []" :columns="scrapeColumns" />
      </UCard>
    </div>
  </div>
</template>