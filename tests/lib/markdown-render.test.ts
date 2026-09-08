import { describe, it, expect } from 'vitest'
import { ref } from 'vue'
import { useMarkdown } from '@/hooks/markdown'

describe('useMarkdown heading anchors', () => {
  it('assigns github-style ids to headings', async () => {
    const app = ref({ appDir: 'app' })
    const key = ref<Uint8Array | null>(null)
    const { render } = useMarkdown(app, key)
    const html = await render('# Table of Contents\n\ntext')
    expect(html).toContain('id="table-of-contents"')
  })

  it('dedupes repeated heading slugs', async () => {
    const app = ref({ appDir: 'app' })
    const key = ref<Uint8Array | null>(null)
    const { render } = useMarkdown(app, key)
    const html = await render('# Same\n\n## Same')
    expect(html).toContain('id="same"')
    expect(html).toContain('id="same-1"')
  })

  it('renders external images untouched', async () => {
    const app = ref({ appDir: 'app' })
    const key = ref<Uint8Array | null>(null)
    const { render } = useMarkdown(app, key)
    const html = await render('![Diagram 1](https://plainapp.app/blog/dlna-cast/diagram-01.svg)')
    expect(html).toContain('src="https://plainapp.app/blog/dlna-cast/diagram-01.svg"')
  })
})
