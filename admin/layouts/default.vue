<template>
  <div class="min-h-screen bg-neutral-50 dark:bg-neutral-900">
    <UContainer class="py-6">
      <div class="flex items-center justify-between mb-8">
        <div class="flex items-center gap-3">
          <UButton icon="i-lucide-menu" color="neutral" variant="ghost" class="lg:hidden" @click="isOpen = true" />
          <h1 class="text-xl font-bold text-neutral-900 dark:text-white">🔒 ScraperBot Admin</h1>
        </div>
        <UButton icon="i-lucide-log-out" color="neutral" variant="ghost" @click="logout" />
      </div>

      <div class="flex gap-6">
        <!-- Sidebar -->
        <UNavigationMenu
          orientation="vertical"
          class="hidden lg:flex w-64 shrink-0"
          :items="items"
        />

        <!-- Mobile drawer -->
        <USlideover v-model:open="isOpen" side="left" class="lg:hidden">
          <template #content>
            <div class="p-4">
              <UNavigationMenu orientation="vertical" :items="items" @click="isOpen = false" />
            </div>
          </template>
        </USlideover>

        <!-- Main content -->
        <div class="flex-1 min-w-0">
          <slot />
        </div>
      </div>
    </UContainer>
  </div>
</template>

<script setup>
const isOpen = ref(false)
const router = useRouter()

const items = [
  { label: 'Dashboard', icon: 'i-lucide-layout-dashboard', to: '/' },
  { label: 'Usuarios', icon: 'i-lucide-users', to: '/users' },
  { label: 'Búsquedas', icon: 'i-lucide-search', to: '/searches' },
  { label: 'Resultados', icon: 'i-lucide-file-text', to: '/results' },
  { label: 'Logs', icon: 'i-lucide-activity', to: '/logs' },
]

function logout() {
  localStorage.removeItem('admin_token')
  router.push('/login')
}
</script>
