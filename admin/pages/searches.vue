<script setup>
definePageMeta({ middleware: "auth" })
const token = useAdminToken()
const { data, pending, error } = useFetch('/api/admin/searches', {
  baseURL: useRuntimeConfig().public.apiBase,
  query: { token },
  server: false
})

const columns = [
  { accessorKey: 'id', header: 'ID' },
  { accessorKey: 'name', header: 'Nombre' },
  { accessorKey: 'search_type', header: 'Tipo' },
  { accessorKey: 'notify_mode', header: 'Notif.' },
  { accessorKey: 'keywords', header: 'Keywords' },
  { accessorKey: 'result_count', header: 'Resultados' },
  { accessorKey: 'created_at', header: 'Creada' },
]
</script>

<template>
  <div>
    <h2 class="text-lg font-semibold mb-4">Búsquedas</h2>
    <div v-if="pending" class="text-neutral-500">Cargando...</div>
    <div v-else-if="error" class="text-red-500">Error cargando datos</div>
    <UTable v-else :data="data || []" :columns="columns" />
  </div>
</template>