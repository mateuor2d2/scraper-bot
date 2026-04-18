<script setup>
definePageMeta({ middleware: "auth" })
const token = useAdminToken()
const { data, pending, error } = useFetch('/api/admin/users', {
  baseURL: useRuntimeConfig().public.apiBase,
  query: { token },
  server: false
})

const columns = [
  { accessorKey: 'telegram_id', header: 'Telegram ID' },
  { accessorKey: 'first_name', header: 'Nombre' },
  { accessorKey: 'username', header: 'Usuario' },
  { accessorKey: 'is_admin', header: 'Admin' },
  { accessorKey: 'search_count', header: 'Búsquedas' },
  { accessorKey: 'created_at', header: 'Registro' },
]
</script>

<template>
  <div>
    <h2 class="text-lg font-semibold mb-4">Usuarios</h2>
    <div v-if="pending" class="text-neutral-500">Cargando...</div>
    <div v-else-if="error" class="text-red-500">Error cargando datos</div>
    <UTable v-else :data="data || []" :columns="columns" />
  </div>
</template>