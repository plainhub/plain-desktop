import { expect, it } from 'vitest'
import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import CreatedTokenDialog from '@/views/settings/sessions/CreatedTokenDialog.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import messages from '@/locales/en-US/sessions'
import { ModalShell, CopyButton } from '../helpers/vue-node-renderer'

it('shows the newly created credential with an explicit one-time reminder', async () => {
  const app = createSSRApp(CreatedTokenDialog, { credential: { clientId: '706c6169-b9ef-8b64-b37d-1162ef232835', token: 'synthetic-secret' } })
  app.config.globalProperties.$t = (key: string) => messages.session_management[key.replace('session_management.', '') as keyof typeof messages.session_management] ?? key
  app.component('VModal', ModalShell)
  app.component('VFilledButton', VFilledButton)
  app.component('VCopyButton', CopyButton)
  const html = await renderToString(app)
  expect(html).toContain(messages.session_management.copy_new_token)
  expect(html).toContain('After closing this window, it will not be shown again.')
  expect(html).toContain('706c6169-b9ef-8b64-b37d-1162ef232835')
  expect(html).toContain('synthetic-secret')
})
