import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: 'স্ক্রিন ক্যাপচার টুল',
      annotationTools: 'টীকা দেওয়ার টুল',
      history: 'ইতিহাস',
      colors: 'রং',
      strokeWidth: 'রেখার প্রস্থ',
      actions: 'ক্যাপচার অ্যাকশন',
      capturedScreen: 'ক্যাপচার করা স্ক্রিন',
      annotations: 'স্ক্রিন ক্যাপচার টীকা',
      resizeSelection: 'নির্বাচনের আকার পরিবর্তন {handle}',
      annotationText: 'টীকার লেখা',
      color: 'রং {color}',
      strokeWidthOption: 'রেখার প্রস্থ {width}',
    },
    tools: { rectangle: 'আয়তক্ষেত্র', ellipse: 'উপবৃত্ত', arrow: 'তীর', pen: 'কলম', text: 'লেখা', mosaic: 'মোজাইক' },
    actions: { undo: 'পূর্বাবস্থায় ফেরান', redo: 'আবার করুন', save: 'সংরক্ষণ', copy: 'কপি', cancel: 'বাতিল', confirm: 'নিশ্চিত করুন', openChatToSend: 'পাঠাতে একটি চ্যাট খুলুন' },
    status: { saving: 'ক্যাপচার সংরক্ষণ করা হচ্ছে…', copying: 'ক্যাপচার কপি করা হচ্ছে…', sending: 'ক্যাপচার পাঠানো হচ্ছে…', cancelling: 'ক্যাপচার বাতিল করা হচ্ছে…' },
    errors: {
      saveFailed: 'ক্যাপচার সংরক্ষণ করা যায়নি। আবার চেষ্টা করুন।',
      copyFailed: 'ক্যাপচার কপি করা যায়নি। আবার চেষ্টা করুন।',
      sendFailed: 'ক্যাপচার পাঠানো যায়নি। আবার চেষ্টা করুন।',
      cancelFailed: 'ক্যাপচার বাতিল করা যায়নি। আবার চেষ্টা করুন।',
    },
  },
} satisfies CaptureLocaleModule
