import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: '화면 캡처 도구',
      annotationTools: '주석 도구',
      history: '기록',
      colors: '색상',
      strokeWidth: '선 두께',
      actions: '캡처 작업',
      capturedScreen: '캡처된 화면',
      annotations: '화면 캡처 주석',
      resizeSelection: '선택 영역 크기 조절 {handle}',
      annotationText: '주석 텍스트',
      color: '색상 {color}',
      strokeWidthOption: '선 두께 {width}',
    },
    tools: { rectangle: '사각형', ellipse: '타원', arrow: '화살표', pen: '펜', text: '텍스트', mosaic: '모자이크' },
    actions: { undo: '실행 취소', redo: '다시 실행', save: '저장', copy: '복사', cancel: '취소', confirm: '확인', openChatToSend: '보낼 채팅을 여세요' },
    status: { saving: '캡처 저장 중…', copying: '캡처 복사 중…', sending: '캡처 전송 중…', cancelling: '캡처 취소 중…' },
    errors: {
      saveFailed: '캡처를 저장할 수 없습니다. 다시 시도하세요.',
      copyFailed: '캡처를 복사할 수 없습니다. 다시 시도하세요.',
      sendFailed: '캡처를 보낼 수 없습니다. 다시 시도하세요.',
      cancelFailed: '캡처를 취소할 수 없습니다. 다시 시도하세요.',
    },
  },
} satisfies CaptureLocaleModule
