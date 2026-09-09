import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: 'Strumenti di cattura schermo',
      annotationTools: 'Strumenti di annotazione',
      history: 'Cronologia',
      colors: 'Colori',
      strokeWidth: 'Spessore tratto',
      actions: 'Azioni di cattura',
      capturedScreen: 'Schermata acquisita',
      annotations: 'Annotazioni della cattura',
      resizeSelection: 'Ridimensiona selezione {handle}',
      annotationText: 'Testo annotazione',
      color: 'Colore {color}',
      strokeWidthOption: 'Spessore tratto {width}',
    },
    tools: { rectangle: 'Rettangolo', ellipse: 'Ellisse', arrow: 'Freccia', pen: 'Penna', text: 'Testo', mosaic: 'Mosaico' },
    actions: { undo: 'Annulla', redo: 'Ripeti', save: 'Salva', copy: 'Copia', cancel: 'Annulla', confirm: 'Conferma', openChatToSend: 'Apri una chat per inviare' },
    status: { saving: 'Salvataggio cattura…', copying: 'Copia cattura…', sending: 'Invio cattura…', cancelling: 'Annullamento cattura…' },
    errors: {
      saveFailed: 'Impossibile salvare la cattura. Riprova.',
      copyFailed: 'Impossibile copiare la cattura. Riprova.',
      sendFailed: 'Impossibile inviare la cattura. Riprova.',
      cancelFailed: 'Impossibile annullare la cattura. Riprova.',
    },
  },
} satisfies CaptureLocaleModule
