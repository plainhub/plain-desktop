import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: 'Hulpmiddelen voor schermopname',
      annotationTools: 'Annotatiehulpmiddelen',
      history: 'Geschiedenis',
      colors: 'Kleuren',
      strokeWidth: 'Lijndikte',
      actions: 'Opnameacties',
      capturedScreen: 'Vastgelegd scherm',
      annotations: 'Annotaties bij schermopname',
      resizeSelection: 'Selectieformaat wijzigen {handle}',
      annotationText: 'Annotatietekst',
      color: 'Kleur {color}',
      strokeWidthOption: 'Lijndikte {width}',
    },
    tools: { rectangle: 'Rechthoek', ellipse: 'Ellips', arrow: 'Pijl', pen: 'Pen', text: 'Tekst', mosaic: 'Mozaïek' },
    actions: { undo: 'Ongedaan maken', redo: 'Opnieuw', save: 'Opslaan', copy: 'Kopiëren', cancel: 'Annuleren', confirm: 'Bevestigen', openChatToSend: 'Open een chat om te verzenden' },
    status: { saving: 'Opname opslaan…', copying: 'Opname kopiëren…', sending: 'Opname verzenden…', cancelling: 'Opname annuleren…' },
    errors: {
      saveFailed: 'De opname kon niet worden opgeslagen. Probeer opnieuw.',
      copyFailed: 'De opname kon niet worden gekopieerd. Probeer opnieuw.',
      sendFailed: 'De opname kon niet worden verzonden. Probeer opnieuw.',
      cancelFailed: 'De opname kon niet worden geannuleerd. Probeer opnieuw.',
    },
  },
} satisfies CaptureLocaleModule
