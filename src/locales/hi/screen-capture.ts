import type { CaptureLocaleModule } from '@/views/screen-capture/capture-localization'

export default {
  screen_capture: {
    a11y: {
      toolbar: 'स्क्रीन कैप्चर टूल',
      annotationTools: 'एनोटेशन टूल',
      history: 'इतिहास',
      colors: 'रंग',
      strokeWidth: 'रेखा की मोटाई',
      actions: 'कैप्चर क्रियाएँ',
      capturedScreen: 'कैप्चर की गई स्क्रीन',
      annotations: 'स्क्रीन कैप्चर एनोटेशन',
      resizeSelection: 'चयन का आकार बदलें {handle}',
      annotationText: 'एनोटेशन टेक्स्ट',
      color: 'रंग {color}',
      strokeWidthOption: 'रेखा की मोटाई {width}',
    },
    tools: { rectangle: 'आयत', ellipse: 'दीर्घवृत्त', arrow: 'तीर', pen: 'पेन', text: 'टेक्स्ट', mosaic: 'मोज़ेक' },
    actions: { undo: 'पूर्ववत करें', redo: 'फिर करें', save: 'सहेजें', copy: 'कॉपी करें', cancel: 'रद्द करें', confirm: 'पुष्टि करें', openChatToSend: 'भेजने के लिए चैट खोलें' },
    status: { saving: 'कैप्चर सहेजा जा रहा है…', copying: 'कैप्चर कॉपी किया जा रहा है…', sending: 'कैप्चर भेजा जा रहा है…', cancelling: 'कैप्चर रद्द किया जा रहा है…' },
    errors: {
      saveFailed: 'कैप्चर सहेजा नहीं जा सका। फिर प्रयास करें।',
      copyFailed: 'कैप्चर कॉपी नहीं किया जा सका। फिर प्रयास करें।',
      sendFailed: 'कैप्चर भेजा नहीं जा सका। फिर प्रयास करें।',
      cancelFailed: 'कैप्चर रद्द नहीं किया जा सका। फिर प्रयास करें।',
    },
  },
} satisfies CaptureLocaleModule
