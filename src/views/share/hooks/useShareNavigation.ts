import { computed, ref } from 'vue'

export function useShareNavigation(load: (relativePath: string) => void) {
  const currentPath = ref('')

  const breadcrumbs = computed(() => {
    if (!currentPath.value) return []
    const segments = currentPath.value.split('/')
    return segments.map((name, i) => ({
      name,
      path: segments.slice(0, i + 1).join('/'),
    }))
  })

  function navigateTo(path: string) {
    if (path === currentPath.value) return
    currentPath.value = path
    load(path)
  }

  function goUp() {
    navigateTo(currentPath.value.substring(0, currentPath.value.lastIndexOf('/')))
  }

  return { currentPath, breadcrumbs, navigateTo, goUp }
}
