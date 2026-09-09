import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: 'Ekran yakalama araçları',
      annotationTools: 'Açıklama araçları',
      history: 'Geçmiş',
      colors: 'Renkler',
      strokeWidth: 'Çizgi kalınlığı',
      actions: 'Yakalama eylemleri',
      capturedScreen: 'Yakalanan ekran',
      annotations: 'Ekran görüntüsü açıklamaları',
      resizeSelection: 'Seçimi yeniden boyutlandır {handle}',
      annotationText: 'Açıklama metni',
      color: 'Renk {color}',
      strokeWidthOption: 'Çizgi kalınlığı {width}',
    },
    tools: { rectangle: 'Dikdörtgen', ellipse: 'Elips', arrow: 'Ok', pen: 'Kalem', text: 'Metin', mosaic: 'Mozaik' },
    actions: { undo: 'Geri al', redo: 'Yinele', save: 'Kaydet', copy: 'Kopyala', cancel: 'İptal', confirm: 'Onayla', openChatToSend: 'Göndermek için bir sohbet açın' },
    status: { saving: 'Görüntü kaydediliyor…', copying: 'Görüntü kopyalanıyor…', sending: 'Görüntü gönderiliyor…', cancelling: 'Görüntü iptal ediliyor…' },
    errors: {
      saveFailed: 'Görüntü kaydedilemedi. Tekrar deneyin.',
      copyFailed: 'Görüntü kopyalanamadı. Tekrar deneyin.',
      sendFailed: 'Görüntü gönderilemedi. Tekrar deneyin.',
      cancelFailed: 'Görüntü iptal edilemedi. Tekrar deneyin.',
    },
  },
} satisfies CaptureLocaleModule
