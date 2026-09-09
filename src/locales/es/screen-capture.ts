import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: 'Herramientas de captura de pantalla',
      annotationTools: 'Herramientas de anotación',
      history: 'Historial',
      colors: 'Colores',
      strokeWidth: 'Ancho del trazo',
      actions: 'Acciones de captura',
      capturedScreen: 'Pantalla capturada',
      annotations: 'Anotaciones de captura de pantalla',
      resizeSelection: 'Cambiar tamaño de la selección {handle}',
      annotationText: 'Texto de anotación',
      color: 'Color {color}',
      strokeWidthOption: 'Ancho del trazo {width}',
    },
    tools: { rectangle: 'Rectángulo', ellipse: 'Elipse', arrow: 'Flecha', pen: 'Lápiz', text: 'Texto', mosaic: 'Mosaico' },
    actions: { undo: 'Deshacer', redo: 'Rehacer', save: 'Guardar', copy: 'Copiar', cancel: 'Cancelar', confirm: 'Confirmar', openChatToSend: 'Abre un chat para enviar' },
    status: { saving: 'Guardando captura…', copying: 'Copiando captura…', sending: 'Enviando captura…', cancelling: 'Cancelando captura…' },
    errors: {
      saveFailed: 'No se pudo guardar la captura. Inténtalo de nuevo.',
      copyFailed: 'No se pudo copiar la captura. Inténtalo de nuevo.',
      sendFailed: 'No se pudo enviar la captura. Inténtalo de nuevo.',
      cancelFailed: 'No se pudo cancelar la captura. Inténtalo de nuevo.',
    },
  },
} satisfies CaptureLocaleModule
