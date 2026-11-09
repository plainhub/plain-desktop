import { FileAction } from '@/lib/api/file-action'
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { revealLocalFile, saveLocalFileAs, saveTextFileAs } from '@/lib/native-file-actions'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { useMarkdown, useSafeMarkdown } from '@/hooks/markdown'
import { fileKindOf } from './file-kind'
import { initMutation, runMutation, writeTextFileGQL } from '@/lib/api/mutation'
import { gqlFetch } from '@/lib/api/gql-client'
import { appGQL } from '@/lib/api/query'
import { download, getFileUrl, tokenToKey } from '@/lib/api/file'
import { filePathFromResourceId } from '@/lib/api/resource-link'
import { managedFileUrl } from '@/lib/api/managed-proxy'
import { isLocalMode } from '@/lib/device/local-mode'
import { getCurrentAuthToken } from '@/lib/device/current'
import { getUrlToken } from '@/lib/api/api'
import { invoke } from '@tauri-apps/api/core'

const isTauri = __IS_TAURI__

interface NativePreview {
  status: number
  headers: Record<string, string>
  content: string
}

export function fileNameFromDecryptedPath(path: string): string {
  if (!path || /^(fid:|app:\/\/|https?:\/\/|\{)/.test(path)) return ''
  return path.split(/[\\/]/).filter(Boolean).pop() ?? ''
}

export function useTextFile() {
  const { t } = useI18n()
  const route = useRoute()
  const router = useRouter()
  const tempStore = useTempStore()
  const { app, urlTokenKey } = storeToRefs(tempStore)

  const isLoggedIn = computed(() => !!getCurrentAuthToken())

  const loading = ref(true)
  const error = ref('')
  const content = ref('')
  const draft = ref('')
  const fileName = ref('')
  const mimeType = ref('')
  const fileSize = ref(0)
  const lastModified = ref('')
  const renderedMarkdown = ref('')
  const showSavedPulse = ref(false)
  let savedPulseTimer: number | null = null

  const { render } = useMarkdown(app, urlTokenKey)
  const { render: renderDocument } = useSafeMarkdown(app, urlTokenKey)
  const isDocumentFile = computed(() => fileKindOf(fileName.value, mimeType.value) === 'doc')

  // Computed
  const isMarkdownFile = computed(() => {
    const name = fileName.value.toLowerCase()
    return name.endsWith('.md') || name.endsWith('.markdown')
  })

  const extension = computed(() => {
    const name = (fileName.value || '').toLowerCase()
    const idx = name.lastIndexOf('.')
    return idx <= 0 ? '' : name.substring(idx + 1)
  })

  const LANG_MAP: Record<string, string> = {
    json: 'json', md: 'markdown', markdown: 'markdown',
    yaml: 'yaml', yml: 'yaml',
    js: 'javascript', mjs: 'javascript', cjs: 'javascript',
    ts: 'typescript', mts: 'typescript', cts: 'typescript',
    go: 'go', css: 'css', html: 'html', htm: 'html',
    sh: 'shell', bash: 'shell', toml: 'toml',
  }
  const language = computed(() => LANG_MAP[extension.value] || 'plaintext')

  const fileId = computed(() => String(route.query.id ?? '').trim())
  const managedMedia = computed(() => {
    const { peer, media } = route.query
    return !isTauri && typeof peer === 'string' && peer && typeof media === 'string' && media ? { peer, media } : undefined
  })
  const decryptedPath = ref('')
  const isEditing = computed(() => route.name === 'text-edit')
  const dirty = computed(() => draft.value !== content.value)
  const displayTitle = computed(() => {
    const base = fileName.value || t('view')
    return isEditing.value && dirty.value ? `${base} *` : base
  })
  const statusText = computed(() => {
    if (!isEditing.value || loading.value || error.value) return ''
    if (saving.value) return t('saving')
    if (showSavedPulse.value) return t('saved')
    return ''
  })
  const canEdit = computed(() => isLoggedIn.value && !!decryptedPath.value && !isDocumentFile.value)
  const showInFinder = computed(() =>
    isLocalMode() && !!decryptedPath.value && !decryptedPath.value.startsWith('fsid:')
  )

  // Mutation
  const { mutate: writeTextFile, loading: saving, onDone: onWriteDone, onError: onWriteError } = initMutation({
    document: writeTextFileGQL,
  })

  // Helpers
  async function ensureUrlTokenKey() {
    // No login gate here: in Tauri local mode `getCurrentAuthToken()` is empty
    // (no bound remote device), yet the local server still serves app.urlToken —
    // without it the file id can never be decrypted back to a path.
    if (urlTokenKey.value) return
    try {
      const r = await gqlFetch(appGQL)
      const newToken = r?.data?.app?.urlToken
      if (newToken) {
        urlTokenKey.value = tokenToKey(newToken)
        tempStore.app = r?.data?.app
      }
    } catch { /* ignore */ }
  }

  function tryDecryptPathFromID(id: string) {
    try {
      const key = isLocalMode() && id.startsWith('c.') ? tokenToKey(getUrlToken()) : urlTokenKey.value
      return key ? filePathFromResourceId(key, id) : ''
    } catch { return '' }
  }

  const applyTextContent = async (textContent: string, resetViewMode: boolean) => {
    content.value = textContent
    draft.value = textContent
    renderedMarkdown.value = ''
    void resetViewMode

    if (isDocumentFile.value) {
      renderedMarkdown.value = await renderDocument(textContent)
    } else if (isMarkdownFile.value) {
      if (isLocalMode() && !route.query.sid) await ensureUrlTokenKey()
      try { renderedMarkdown.value = await render(textContent) } catch { /* fallback to raw */ }
    }
  }

  const fetchTextContent = async () => {
    try {
      loading.value = true
      error.value = ''
      const managed = managedMedia.value
      if (!fileId.value && !managed) { error.value = t('invalid_file_id'); return }
      if (managed) await ensureUrlTokenKey()
      const key = urlTokenKey.value
      if (managed && !key) { error.value = t('access_denied'); return }
      const id = fileId.value
      const native = isLocalMode() && !route.query.sid && /^(c|g)\./.test(id)
        ? await invoke<NativePreview>('native_file_preview', { id })
        : undefined
      const response = native ?? await fetch(managed && key
        ? managedFileUrl(managed.peer, key, { type: 'MEDIA', id: managed.media })
        : getFileUrl(id))
      const { status } = response
      if (status < 200 || status >= 300) {
        error.value = status === 404 ? t('file_not_found')
          : status === 403 ? t('access_denied')
          : t('failed_to_load_file')
        return
      }

      const headers = new Headers(response.headers)
      mimeType.value = (headers.get('content-type') || '').split(';')[0].trim().toLowerCase()
      const cd = headers.get('content-disposition')
      if (cd) {
        const match = cd.match(/filename="?([^"]+)"?/)
        if (match) try { fileName.value = decodeURIComponent(match[1]).replace(/[/\\:*?"<>|]+/g, '_') } catch { /* ignore */ }
      }
      const cl = headers.get('content-length')
      if (cl) fileSize.value = parseInt(cl)
      const lm = headers.get('last-modified')
      if (lm) lastModified.value = lm

      await applyTextContent('content' in response ? response.content : await response.text(), true)
    } catch { error.value = t('network_error') } finally { loading.value = false }
  }

  const retry = () => fetchTextContent()
  const openEditor = () => { if (canEdit.value) router.push({ name: 'text-edit', query: { id: fileId.value } }) }
  const openViewer = () => { router.push({ name: 'text-file', query: { id: fileId.value } }) }

  const revealInFinder = () => {
    revealLocalFile(decryptedPath.value).catch((err) => console.error('reveal file failed', err))
  }

  const downloadFile = () => {
    const managed = managedMedia.value
    if (managed && urlTokenKey.value) {
      download(managedFileUrl(managed.peer, urlTokenKey.value, { type: 'MEDIA', id: managed.media }, FileAction.Download), fileName.value)
      return
    }
    if (isDocumentFile.value) {
      if (isLocalMode() && decryptedPath.value) {
        saveLocalFileAs(decryptedPath.value, fileName.value).catch((err) => console.error('document export failed', err))
        return
      }
      download(getFileUrl(fileId.value, '', FileAction.Download), fileName.value)
      return
    }
    if (!content.value || !fileName.value) return
    // WKWebView ignores <a download>, so hand the already-fetched content to
    // the native save dialog instead of creating a blob link.
    if (isTauri) {
      saveTextFileAs(fileName.value, content.value).catch((err) => console.error('text export failed', err))
      return
    }
    const blob = new Blob([content.value], { type: 'text/plain;charset=utf-8' })
    const url = window.URL.createObjectURL(blob)
    const link = document.createElement('a')
    link.href = url
    link.download = fileName.value
    document.body.appendChild(link)
    link.click()
    document.body.removeChild(link)
    window.URL.revokeObjectURL(url)
  }

  async function save() {
    if (!canEdit.value || !isEditing.value || !decryptedPath.value || saving.value || !dirty.value) return
    const ok = await runMutation(writeTextFile, { path: decryptedPath.value, content: draft.value, overwrite: true })
    if (!ok) return
    await applyTextContent(draft.value, false)
    showSavedPulse.value = true
    if (savedPulseTimer) window.clearTimeout(savedPulseTimer)
    savedPulseTimer = window.setTimeout(() => { showSavedPulse.value = false; savedPulseTimer = null }, 1500)
  }

  function onKeyDown(e: KeyboardEvent) {
    if (!isEditing.value || !(e.ctrlKey || e.metaKey) || e.altKey) return
    if (e.key !== 's' && e.key !== 'S') return
    e.preventDefault(); e.stopPropagation(); save()
  }

  // Lifecycle
  watch([fileId, managedMedia], async ([id, managed]) => {
    decryptedPath.value = ''
    fileName.value = ''
    mimeType.value = ''
    if (!id && !managed) return
    if (!managed && !route.query.sid) {
      if (!isLocalMode() || !id.startsWith('c.')) await ensureUrlTokenKey()
      decryptedPath.value = tryDecryptPathFromID(id)
      fileName.value = fileNameFromDecryptedPath(decryptedPath.value)
    }
    fetchTextContent()
  }, { immediate: true })

  onMounted(() => { window.addEventListener('keydown', onKeyDown, { capture: true }) })
  onBeforeUnmount(() => {
    window.removeEventListener('keydown', onKeyDown, { capture: true } as any)
    if (savedPulseTimer) window.clearTimeout(savedPulseTimer)
  })
  watch([fileName, dirty, isEditing], () => { document.title = displayTitle.value })

  return {
    loading, error, content, draft, fileName, mimeType, fileSize, lastModified,
    renderedMarkdown, saving,
    isMarkdownFile, language,
    isEditing, dirty, displayTitle, statusText, canEdit, showSavedPulse, showInFinder,
    retry, openEditor, openViewer,
    downloadFile, revealInFinder, save, isLoggedIn,
  }
}
