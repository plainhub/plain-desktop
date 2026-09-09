import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: 'Ferramentas de captura de tela',
      annotationTools: 'Ferramentas de anotação',
      history: 'Histórico',
      colors: 'Cores',
      strokeWidth: 'Espessura do traço',
      actions: 'Ações da captura',
      capturedScreen: 'Tela capturada',
      annotations: 'Anotações da captura de tela',
      resizeSelection: 'Redimensionar seleção {handle}',
      annotationText: 'Texto da anotação',
      color: 'Cor {color}',
      strokeWidthOption: 'Espessura do traço {width}',
    },
    tools: { rectangle: 'Retângulo', ellipse: 'Elipse', arrow: 'Seta', pen: 'Caneta', text: 'Texto', mosaic: 'Mosaico' },
    actions: { undo: 'Desfazer', redo: 'Refazer', save: 'Salvar', copy: 'Copiar', cancel: 'Cancelar', confirm: 'Confirmar', openChatToSend: 'Abra uma conversa para enviar' },
    status: { saving: 'Salvando captura…', copying: 'Copiando captura…', sending: 'Enviando captura…', cancelling: 'Cancelando captura…' },
    errors: {
      saveFailed: 'Não foi possível salvar a captura. Tente novamente.',
      copyFailed: 'Não foi possível copiar a captura. Tente novamente.',
      sendFailed: 'Não foi possível enviar a captura. Tente novamente.',
      cancelFailed: 'Não foi possível cancelar a captura. Tente novamente.',
    },
  },
} satisfies CaptureLocaleModule
