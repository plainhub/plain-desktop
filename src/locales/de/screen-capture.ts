import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: 'Werkzeuge für Bildschirmaufnahmen',
      annotationTools: 'Anmerkungswerkzeuge',
      history: 'Verlauf',
      colors: 'Farben',
      strokeWidth: 'Strichstärke',
      actions: 'Aufnahmeaktionen',
      capturedScreen: 'Aufgenommener Bildschirm',
      annotations: 'Anmerkungen zur Bildschirmaufnahme',
      resizeSelection: 'Auswahlgröße ändern {handle}',
      annotationText: 'Anmerkungstext',
      color: 'Farbe {color}',
      strokeWidthOption: 'Strichstärke {width}',
    },
    tools: { rectangle: 'Rechteck', ellipse: 'Ellipse', arrow: 'Pfeil', pen: 'Stift', text: 'Text', mosaic: 'Mosaik' },
    actions: { undo: 'Rückgängig', redo: 'Wiederholen', save: 'Speichern', copy: 'Kopieren', cancel: 'Abbrechen', confirm: 'Bestätigen', openChatToSend: 'Zum Senden einen Chat öffnen' },
    status: { saving: 'Aufnahme wird gespeichert…', copying: 'Aufnahme wird kopiert…', sending: 'Aufnahme wird gesendet…', cancelling: 'Aufnahme wird abgebrochen…' },
    errors: {
      saveFailed: 'Die Aufnahme konnte nicht gespeichert werden. Bitte erneut versuchen.',
      copyFailed: 'Die Aufnahme konnte nicht kopiert werden. Bitte erneut versuchen.',
      sendFailed: 'Die Aufnahme konnte nicht gesendet werden. Bitte erneut versuchen.',
      cancelFailed: 'Die Aufnahme konnte nicht abgebrochen werden. Bitte erneut versuchen.',
    },
  },
} satisfies CaptureLocaleModule
