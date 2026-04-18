export function useAdminToken() {
  if (typeof window === 'undefined') return ''
  return localStorage.getItem('admin_token') || ''
}
