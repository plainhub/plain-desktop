import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: '截图工具',
      annotationTools: '标注工具',
      history: '历史记录',
      colors: '颜色',
      strokeWidth: '线条粗细',
      actions: '截图操作',
      capturedScreen: '已截取的屏幕',
      annotations: '截图标注',
      resizeSelection: '调整选区大小 {handle}',
      annotationText: '标注文字',
      color: '颜色 {color}',
      strokeWidthOption: '线条粗细 {width}',
    },
    tools: { rectangle: '矩形', ellipse: '椭圆', arrow: '箭头', pen: '画笔', text: '文字', mosaic: '马赛克' },
    actions: { undo: '撤销', redo: '重做', save: '保存', copy: '复制', cancel: '取消', confirm: '确认', openChatToSend: '打开聊天后发送' },
    status: { saving: '正在保存截图…', copying: '正在复制截图…', sending: '正在发送截图…', cancelling: '正在取消截图…' },
    errors: {
      saveFailed: '无法保存截图，请重试。',
      copyFailed: '无法复制截图，请重试。',
      sendFailed: '无法发送截图，请重试。',
      cancelFailed: '无法取消截图，请重试。',
    },
  },
} satisfies CaptureLocaleModule
