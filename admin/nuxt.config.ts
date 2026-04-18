export default defineNuxtConfig({
  devtools: { enabled: true },
  modules: ['@nuxt/ui'],
  ui: {
    colors: {
      primary: 'blue',
      neutral: 'zinc'
    }
  },
  runtimeConfig: {
    public: {
      apiBase: process.env.NUXT_PUBLIC_API_BASE || ''
    }
  }
})
