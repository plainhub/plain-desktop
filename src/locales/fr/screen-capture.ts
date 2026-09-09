import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: 'Outils de capture d’écran',
      annotationTools: 'Outils d’annotation',
      history: 'Historique',
      colors: 'Couleurs',
      strokeWidth: 'Épaisseur du trait',
      actions: 'Actions de capture',
      capturedScreen: 'Écran capturé',
      annotations: 'Annotations de la capture d’écran',
      resizeSelection: 'Redimensionner la sélection {handle}',
      annotationText: 'Texte d’annotation',
      color: 'Couleur {color}',
      strokeWidthOption: 'Épaisseur du trait {width}',
    },
    tools: { rectangle: 'Rectangle', ellipse: 'Ellipse', arrow: 'Flèche', pen: 'Stylo', text: 'Texte', mosaic: 'Mosaïque' },
    actions: { undo: 'Annuler', redo: 'Rétablir', save: 'Enregistrer', copy: 'Copier', cancel: 'Annuler', confirm: 'Confirmer', openChatToSend: 'Ouvrez une discussion pour envoyer' },
    status: { saving: 'Enregistrement de la capture…', copying: 'Copie de la capture…', sending: 'Envoi de la capture…', cancelling: 'Annulation de la capture…' },
    errors: {
      saveFailed: 'Impossible d’enregistrer la capture. Réessayez.',
      copyFailed: 'Impossible de copier la capture. Réessayez.',
      sendFailed: 'Impossible d’envoyer la capture. Réessayez.',
      cancelFailed: 'Impossible d’annuler la capture. Réessayez.',
    },
  },
} satisfies CaptureLocaleModule
