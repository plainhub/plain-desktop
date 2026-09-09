import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: '螢幕擷取工具',
      annotationTools: '標註工具',
      history: '歷史記錄',
      colors: '顏色',
      strokeWidth: '線條粗細',
      actions: '擷取操作',
      capturedScreen: '已擷取的螢幕',
      annotations: '螢幕擷取標註',
      resizeSelection: '調整選取範圍大小 {handle}',
      annotationText: '標註文字',
      color: '顏色 {color}',
      strokeWidthOption: '線條粗細 {width}',
    },
    tools: { rectangle: '矩形', ellipse: '橢圓', arrow: '箭頭', pen: '畫筆', text: '文字', mosaic: '馬賽克' },
    actions: { undo: '復原', redo: '重做', save: '儲存', copy: '複製', cancel: '取消', confirm: '確認', openChatToSend: '開啟聊天後傳送' },
    status: { saving: '正在儲存擷取畫面…', copying: '正在複製擷取畫面…', sending: '正在傳送擷取畫面…', cancelling: '正在取消擷取…' },
    errors: {
      saveFailed: '無法儲存擷取畫面，請再試一次。',
      copyFailed: '無法複製擷取畫面，請再試一次。',
      sendFailed: '無法傳送擷取畫面，請再試一次。',
      cancelFailed: '無法取消擷取，請再試一次。',
    },
  },
} satisfies CaptureLocaleModule
