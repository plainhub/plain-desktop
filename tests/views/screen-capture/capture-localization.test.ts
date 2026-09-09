import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'
import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'
import { CAPTURE_LOCALE_CODES, captureMessagesForLanguages } from '@/views/screen-capture/capture-localization'
import bootstrapSource from '@/views/screen-capture/bootstrap.ts?raw'
import localizationSource from '@/views/screen-capture/capture-localization.ts?raw'
import overlaySource from '@/views/screen-capture/ScreenCaptureOverlay.vue?raw'
import toolbarSource from '@/views/screen-capture/ScreenCaptureToolbar.vue?raw'
import ScreenCaptureOverlay from '@/views/screen-capture/ScreenCaptureOverlay.vue'
import ScreenCaptureToolbar from '@/views/screen-capture/ScreenCaptureToolbar.vue'

vi.mock('@/views/image-editor/pixi/PixiEditorRenderer', () => ({
  PixiEditorRenderer: class {
    isReady = false
    async init() {
      this.isReady = true
    }
    resize() {}
    setViewport() {}
    sync() {}
    paint() {
      return this.isReady
    }
    destroy() {
      this.isReady = false
    }
  },
}))

const localeModules = import.meta.glob<CaptureLocaleModule>('@/locales/*/screen-capture.ts', {
  eager: true,
  import: 'default',
})

function localeCode(path: string): string {
  const match = path.match(/\/locales\/([^/]+)\/screen-capture\.ts$/)
  if (!match) throw new Error(`unexpected capture locale path: ${path}`)
  return match[1]!
}

function leafKeys(value: unknown, prefix = ''): string[] {
  if (!value || typeof value !== 'object') return [prefix]
  return Object.entries(value)
    .flatMap(([key, child]) => leafKeys(child, prefix ? `${prefix}.${key}` : key))
    .sort()
}

function leafStrings(value: unknown): string[] {
  if (typeof value === 'string') return [value]
  if (!value || typeof value !== 'object') return []
  return Object.values(value).flatMap(leafStrings)
}

function createFrame(width = 100, height = 80): ImageData {
  return new ImageData(new Uint8ClampedArray(width * height * 4), width, height)
}

function frameRect(): DOMRect {
  return { x: 0, y: 0, left: 0, top: 0, right: 100, bottom: 80, width: 100, height: 80, toJSON: () => ({}) }
}

async function pointer(element: Element, type: string, x: number, y: number, pointerId = 1) {
  element.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y, pointerId }))
  await nextTick()
}

describe('capture-only localization', () => {
  it('ships the exact same capture message contract in all 17 supported locales', () => {
    const modulesByCode = Object.fromEntries(Object.entries(localeModules).map(([path, module]) => [localeCode(path), module]))
    expect(Object.keys(modulesByCode).sort()).toEqual([...CAPTURE_LOCALE_CODES].sort())

    const expectedKeys = leafKeys(modulesByCode['en-US'])
    for (const [code, messages] of Object.entries(modulesByCode)) {
      expect(leafKeys(messages), code).toEqual(expectedKeys)
      for (const [key, value] of Object.entries(messages).flatMap(function flatten([key, value]): Array<[string, unknown]> {
        if (!value || typeof value !== 'object') return [[key, value]]
        return Object.entries(value).flatMap(([childKey, child]) => flatten([`${key}.${childKey}`, child]))
      })) {
        expect(typeof value, `${code}:${key}`).toBe('string')
        expect(String(value).trim(), `${code}:${key}`).not.toBe('')
      }
    }
  })

  it('matches exact and base browser languages without loading full-app preferences', () => {
    expect(captureMessagesForLanguages(['es-MX']).actions.save).toBe('Guardar')
    expect(captureMessagesForLanguages(['zh-Hant-HK']).actions.save).toBe('儲存')
    expect(captureMessagesForLanguages(['unknown']).actions.save).toBe('Save')
  })

  it('keeps the capture bootstrap isolated from the full app localization stack', () => {
    const capturePath = `${bootstrapSource}\n${localizationSource}`
    for (const forbidden of ['@/lib/prefs', '@/plugins/i18n', 'vue-i18n', 'vue-router', 'pinia', '@/stores/', 'gql-client']) {
      expect(capturePath).not.toContain(forbidden)
    }
  })

  it('renders localized toolbar labels from an injected capture-only dictionary', () => {
    const messages = captureMessagesForLanguages(['es'])
    const wrapper = mount(ScreenCaptureToolbar, {
      props: {
        activeTool: null,
        color: '#ef4444',
        strokeWidth: 4,
        canUndo: false,
        canRedo: false,
        busy: false,
        canConfirm: false,
        messages,
      },
    })

    expect(wrapper.attributes('aria-label')).toBe('Herramientas de captura de pantalla')
    expect(wrapper.get('[data-tool="rect"]').attributes('aria-label')).toBe('Rectángulo')
    expect(wrapper.get('[data-action="save"]').attributes('aria-label')).toBe('Guardar')
    expect(wrapper.get('[data-action="confirm"]').attributes('title')).toBe('Abre un chat para enviar')
    expect(wrapper.get('[data-testid="capture-confirm-tooltip-host"]').attributes('tabindex')).toBe('0')
    expect(wrapper.get('[data-action="confirm"]').attributes('aria-describedby')).toBe('capture-confirm-disabled-reason')
    expect(wrapper.get('#capture-confirm-disabled-reason').attributes('role')).toBe('tooltip')
    expect(wrapper.get('#capture-confirm-disabled-reason').text()).toBe('Abre un chat para enviar')
    wrapper.unmount()
  })

  it('localizes retryable overlay failures instead of exposing callback diagnostics', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    const wrapper = mount(ScreenCaptureOverlay, {
      attachTo: document.body,
      props: {
        frame: createFrame(),
        messages: captureMessagesForLanguages(['es']),
        onExport: vi.fn(async () => {
          throw new Error('native clipboard diagnostic')
        }),
        onCancel: vi.fn(async () => {}),
      },
    })
    await nextTick()
    const stage = wrapper.get('[data-testid="capture-stage"]')
    Object.defineProperty(stage.element, 'getBoundingClientRect', { value: frameRect })
    await pointer(stage.element, 'pointerdown', 10, 10)
    await pointer(stage.element, 'pointermove', 70, 60)
    await pointer(stage.element, 'pointerup', 70, 60)
    await wrapper.get('[data-action="copy"]').trigger('click')

    await vi.waitFor(() => expect(wrapper.get('[role="alert"]').text()).toBe('No se pudo copiar la captura. Inténtalo de nuevo.'))
    expect(wrapper.text()).not.toContain('native clipboard diagnostic')
    wrapper.unmount()
  })

  it('keeps user-facing English out of the overlay component sources', () => {
    const componentSource = `${toolbarSource}\n${overlaySource}`
    for (const english of leafStrings(captureMessagesForLanguages(['en-US']))) {
      expect(componentSource).not.toContain(JSON.stringify(english))
      expect(componentSource).not.toContain(`'${english}'`)
    }
    for (const english of [
      'Screen capture tools',
      'Annotation tools',
      'Captured screen',
      'Screen capture annotations',
      'Resize selection',
      'Annotation text',
      'Open a chat to send',
      'Unable to export capture',
    ]) {
      expect(componentSource).not.toContain(english)
    }
  })
})
