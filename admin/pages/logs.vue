<script setup>
definePageMeta({ middleware: "auth" })
const token = useAdminToken()
const { data, pending, error } = useFetch('/api/admin/logs', {
  baseURL: useRuntimeConfig().public.apiBase,
  query: { token },
  server: false
})

const columns = [
  { accessorKey: 'config_name', header: 'Búsqueda' },
  { accessorKey: 'status', header: 'Estado' },
  { accessorKey: 'items_found', header: 'Items' },
  { accessorKey: 'error_message', header: 'Error' },
  { accessorKey: 'duration_ms', header: 'Duración (ms)' },
  { accessorKey: 'created_at', header: 'Fecha' },
]
</script>

<template>
  <div>
    <h2 class="text-lg font-semibold mb-4">Logs de scraping</h2>
    <div v-if="pending" class="text-neutral-500">Cargando...</div>
    <div v-else-if="error" class="text-red-500">Error cargando datos</div>
    <UTable v-else :data="data || []" :columns="columns" />
  </div>
</template>