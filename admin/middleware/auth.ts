export default defineNuxtRouteMiddleware(() => {
  if (typeof window === 'undefined') return
  const token = localStorage.getItem('admin_token')
  if (!token) {
    return navigateTo('/login')
  }
})
