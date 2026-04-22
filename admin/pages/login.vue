<template>
  <div class="min-h-screen flex items-center justify-center bg-neutral-50 dark:bg-neutral-900">
    <UCard class="w-full max-w-sm">
      <template #header>
        <h2 class="text-lg font-semibold">🔒 Admin ScraperBot</h2>
      </template>
      <form @submit.prevent="doLogin" class="space-y-4">
        <UFormField label="Token de admin">
          <UInput v-model="token" type="password" placeholder="ADMIN_TOKEN o tu telegram_id" class="w-full" />
        </UFormField>
        <UButton type="submit" color="primary" class="w-full justify-center" :loading="loading">
          Entrar
        </UButton>
      </form>
    </UCard>
  </div>
</template>

<script setup>
definePageMeta({ layout: false })
const token = ref('')
const loading = ref(false)
const router = useRouter()

onMounted(() => {
  if (localStorage.getItem('admin_token')) {
    router.push('/')
  }
})

function doLogin() {
  loading.value = true
  localStorage.setItem('admin_token', token.value)
  router.push('/')
  loading.value = false
}
</script>
