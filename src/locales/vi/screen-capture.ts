import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: 'Công cụ chụp màn hình',
      annotationTools: 'Công cụ chú thích',
      history: 'Lịch sử',
      colors: 'Màu sắc',
      strokeWidth: 'Độ dày nét',
      actions: 'Thao tác ảnh chụp',
      capturedScreen: 'Màn hình đã chụp',
      annotations: 'Chú thích ảnh chụp màn hình',
      resizeSelection: 'Đổi kích thước vùng chọn {handle}',
      annotationText: 'Văn bản chú thích',
      color: 'Màu {color}',
      strokeWidthOption: 'Độ dày nét {width}',
    },
    tools: { rectangle: 'Hình chữ nhật', ellipse: 'Hình elip', arrow: 'Mũi tên', pen: 'Bút', text: 'Văn bản', mosaic: 'Khảm' },
    actions: { undo: 'Hoàn tác', redo: 'Làm lại', save: 'Lưu', copy: 'Sao chép', cancel: 'Hủy', confirm: 'Xác nhận', openChatToSend: 'Mở cuộc trò chuyện để gửi' },
    status: { saving: 'Đang lưu ảnh chụp…', copying: 'Đang sao chép ảnh chụp…', sending: 'Đang gửi ảnh chụp…', cancelling: 'Đang hủy ảnh chụp…' },
    errors: {
      saveFailed: 'Không thể lưu ảnh chụp. Hãy thử lại.',
      copyFailed: 'Không thể sao chép ảnh chụp. Hãy thử lại.',
      sendFailed: 'Không thể gửi ảnh chụp. Hãy thử lại.',
      cancelFailed: 'Không thể hủy ảnh chụp. Hãy thử lại.',
    },
  },
} satisfies CaptureLocaleModule
