import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: 'திரைப்பிடிப்பு கருவிகள்',
      annotationTools: 'குறிப்புக் கருவிகள்',
      history: 'வரலாறு',
      colors: 'நிறங்கள்',
      strokeWidth: 'கோட்டு அகலம்',
      actions: 'பிடிப்பு செயல்கள்',
      capturedScreen: 'பிடிக்கப்பட்ட திரை',
      annotations: 'திரைப்பிடிப்பு குறிப்புகள்',
      resizeSelection: 'தேர்வின் அளவை மாற்று {handle}',
      annotationText: 'குறிப்பு உரை',
      color: 'நிறம் {color}',
      strokeWidthOption: 'கோட்டு அகலம் {width}',
    },
    tools: { rectangle: 'செவ்வகம்', ellipse: 'நீள்வட்டம்', arrow: 'அம்பு', pen: 'பேனா', text: 'உரை', mosaic: 'மொசைக்' },
    actions: { undo: 'செயல்தவிர்', redo: 'மீண்டும் செய்', save: 'சேமி', copy: 'நகலெடு', cancel: 'ரத்துசெய்', confirm: 'உறுதிசெய்', openChatToSend: 'அனுப்ப ஒரு அரட்டையைத் திறக்கவும்' },
    status: { saving: 'பிடிப்பு சேமிக்கப்படுகிறது…', copying: 'பிடிப்பு நகலெடுக்கப்படுகிறது…', sending: 'பிடிப்பு அனுப்பப்படுகிறது…', cancelling: 'பிடிப்பு ரத்துசெய்யப்படுகிறது…' },
    errors: {
      saveFailed: 'பிடிப்பைச் சேமிக்க முடியவில்லை. மீண்டும் முயலவும்.',
      copyFailed: 'பிடிப்பை நகலெடுக்க முடியவில்லை. மீண்டும் முயலவும்.',
      sendFailed: 'பிடிப்பை அனுப்ப முடியவில்லை. மீண்டும் முயலவும்.',
      cancelFailed: 'பிடிப்பை ரத்துசெய்ய முடியவில்லை. மீண்டும் முயலவும்.',
    },
  },
} satisfies CaptureLocaleModule
