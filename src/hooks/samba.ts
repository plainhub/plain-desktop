import { ref, type Ref } from 'vue'
import toast from '@/components/toaster'
import { useI18n } from 'vue-i18n'
import { initLazyQuery } from '@/lib/api/query'
import { initMutation } from '@/lib/api/mutation'
import { sambaSettingsGQL } from '@/lib/api/query'
import { setSambaSettingsGQL, setSambaUserPasswordGQL } from '@/lib/api/mutation'

export type SambaShareAuth = 'GUEST' | 'PASSWORD'

export interface SambaShare {
    name: string
    sharePath: string
    auth: SambaShareAuth
    readOnly: boolean
}

export interface SambaSettings {
    enabled: boolean
    username: string
    hasPassword: boolean
    shares: SambaShare[]
    serviceName: string
    serviceActive: boolean
    serviceEnabled: boolean
}

type SambaSettingsResult = {
    settings: Ref<SambaSettings | null>
    loading: Ref<boolean>
    saving: Ref<boolean>
    toggling: Ref<boolean>
    passwordSaving: Ref<boolean>
    fetch: () => void
    saveSettings: (input: { enabled: boolean; shares: SambaShare[] }) => Promise<boolean>
    setEnabled: (enabled: boolean) => Promise<boolean>
    setUserPassword: (password: string) => Promise<boolean>
}

let sambaSettingsCache: SambaSettingsResult | null = null

export const useSambaSettings = (): SambaSettingsResult => {
    if (sambaSettingsCache) return sambaSettingsCache

    const { t } = useI18n()
    const settings = ref<SambaSettings | null>(null)

    const { fetch, loading } = initLazyQuery<{ sambaSettings: SambaSettings }>({
        document: sambaSettingsGQL,
        variables: () => ({}),
        handle: (data, error) => {
            if (error) {
                toast(t(error), 'error')
                return
            }
            settings.value = data?.sambaSettings ?? null
        },
    })

    // `mutate` toasts errors itself (initMutation handleError) and resolves
    // undefined on failure — a non-undefined result means success.
    const { mutate: saveMutation, loading: saving } = initMutation({
        document: setSambaSettingsGQL,
    })

    const { mutate: toggleMutation, loading: toggling } = initMutation({
        document: setSambaSettingsGQL,
    })

    const { mutate: setPasswordMutation, loading: passwordSaving } = initMutation({
        document: setSambaUserPasswordGQL,
    })

    const saveSettings = async (input: { enabled: boolean; shares: SambaShare[] }) => {
        const r = await saveMutation({
            input: {
                enabled: !!input.enabled,
                shares: (input.shares ?? []).map((s) => ({
                    name: String(s.name || ''),
                    sharePath: String(s.sharePath || ''),
                    auth: s.auth,
                    readOnly: !!s.readOnly,
                })),
            },
        })
        const ok = r != null
        if (ok) fetch()
        return ok
    }

    const setEnabled = async (enabled: boolean) => {
        const current = settings.value
        if (!current) return false

        const r = await toggleMutation({
            input: {
                enabled: !!enabled,
                shares: (current.shares ?? []).map((s) => ({
                    name: String(s.name || ''),
                    sharePath: String(s.sharePath || ''),
                    auth: s.auth,
                    readOnly: !!s.readOnly,
                })),
            },
        })
        const ok = r != null
        if (ok) fetch()
        return ok
    }

    const setUserPassword = async (password: string) => {
        const r = await setPasswordMutation({ password: String(password || '') })
        const ok = r != null
        if (ok) fetch()
        return ok
    }

    sambaSettingsCache = { settings, loading, saving, toggling, passwordSaving, fetch, saveSettings, setEnabled, setUserPassword }
    sambaSettingsCache.fetch()
    return sambaSettingsCache
}
