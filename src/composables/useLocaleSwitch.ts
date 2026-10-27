import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { loadLocaleMessages, SUPPORTED_LOCALES } from '@/plugins/i18n'
import { setLocale as setTimeagoLocale } from '@/lib/timeago'
import { set as prefsSet } from '@/lib/prefs/appearance'
import { applyMenuLabels } from '@/lib/app-menu'
import { updateDocumentTitle } from '@/plugins/router'

/**
 * Locale switcher.
 *
 * Mirrors plainapp-app's `useLocaleSwitch`: keeps a `switchingLocale` ref that
 * the header overlay reads, awaits the async locale chunk load, and only
 * resolves after the message map is registered with vue-i18n.
 */
export function useLocaleSwitch() {
  const { locale } = useI18n()

  const switchingLocale = ref('')

  const availableLocales = computed(() => SUPPORTED_LOCALES.slice())

  const currentLocaleName = computed(
    () => availableLocales.value.find((l) => l.code === locale.value)?.name ?? locale.value,
  )

  async function handleLocaleSwitch(code: string): Promise<void> {
    if (code === locale.value) return
    const name = availableLocales.value.find((l) => l.code === code)?.name ?? code
    switchingLocale.value = name
    try {
      await Promise.all([loadLocaleMessages(code), setTimeagoLocale(code)])
      locale.value = code
      await prefsSet('locale', code)
      updateDocumentTitle()
      applyMenuLabels()
    } finally {
      switchingLocale.value = ''
    }
  }

  return {
    locale,
    availableLocales,
    currentLocaleName,
    switchingLocale,
    handleLocaleSwitch,
  }
}
