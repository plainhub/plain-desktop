import type { CaptureMessages } from '../capture-localization'

export default {
  a11y: {
    toolbar: 'Screen capture tools',
    annotationTools: 'Annotation tools',
    history: 'History',
    colors: 'Colors',
    strokeWidth: 'Stroke width',
    actions: 'Capture actions',
    capturedScreen: 'Captured screen',
    annotations: 'Screen capture annotations',
    resizeSelection: 'Resize selection {handle}',
    annotationText: 'Annotation text',
    color: 'Color {color}',
    strokeWidthOption: 'Stroke width {width}',
  },
  tools: { rectangle: 'Rectangle', ellipse: 'Ellipse', arrow: 'Arrow', pen: 'Pen', text: 'Text', mosaic: 'Mosaic' },
  actions: { undo: 'Undo', redo: 'Redo', save: 'Save', copy: 'Copy', cancel: 'Cancel', confirm: 'Confirm', openChatToSend: 'Open a chat to send' },
  status: { saving: 'Saving capture…', copying: 'Copying capture…', sending: 'Sending capture…', canceling: 'Canceling capture…' },
  errors: {
    saveFailed: 'Could not save the capture. Try again.',
    copyFailed: 'Could not copy the capture. Try again.',
    sendFailed: 'Could not send the capture. Try again.',
    cancelFailed: 'Could not cancel the capture. Try again.',
  },
} satisfies CaptureMessages
