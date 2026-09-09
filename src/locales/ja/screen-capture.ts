import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: '画面キャプチャツール',
      annotationTools: '注釈ツール',
      history: '履歴',
      colors: '色',
      strokeWidth: '線の太さ',
      actions: 'キャプチャ操作',
      capturedScreen: 'キャプチャした画面',
      annotations: '画面キャプチャの注釈',
      resizeSelection: '選択範囲のサイズ変更 {handle}',
      annotationText: '注釈テキスト',
      color: '色 {color}',
      strokeWidthOption: '線の太さ {width}',
    },
    tools: { rectangle: '長方形', ellipse: '楕円', arrow: '矢印', pen: 'ペン', text: 'テキスト', mosaic: 'モザイク' },
    actions: { undo: '元に戻す', redo: 'やり直す', save: '保存', copy: 'コピー', cancel: 'キャンセル', confirm: '確定', openChatToSend: '送信するチャットを開いてください' },
    status: { saving: 'キャプチャを保存中…', copying: 'キャプチャをコピー中…', sending: 'キャプチャを送信中…', cancelling: 'キャプチャをキャンセル中…' },
    errors: {
      saveFailed: 'キャプチャを保存できませんでした。もう一度お試しください。',
      copyFailed: 'キャプチャをコピーできませんでした。もう一度お試しください。',
      sendFailed: 'キャプチャを送信できませんでした。もう一度お試しください。',
      cancelFailed: 'キャプチャをキャンセルできませんでした。もう一度お試しください。',
    },
  },
} satisfies CaptureLocaleModule
