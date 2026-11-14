import type { CaptureMessages } from '../capture-localization'

export default {
  a11y: {
    toolbar: 'Инструменты снимка экрана',
    annotationTools: 'Инструменты аннотаций',
    history: 'История',
    colors: 'Цвета',
    strokeWidth: 'Толщина линии',
    actions: 'Действия со снимком',
    capturedScreen: 'Снимок экрана',
    annotations: 'Аннотации снимка экрана',
    resizeSelection: 'Изменить размер области {handle}',
    annotationText: 'Текст аннотации',
    color: 'Цвет {color}',
    strokeWidthOption: 'Толщина линии {width}',
  },
  tools: { rectangle: 'Прямоугольник', ellipse: 'Эллипс', arrow: 'Стрелка', pen: 'Перо', text: 'Текст', mosaic: 'Мозаика' },
  actions: { undo: 'Отменить', redo: 'Повторить', save: 'Сохранить', copy: 'Копировать', cancel: 'Отмена', confirm: 'Подтвердить', openChatToSend: 'Откройте чат для отправки' },
  status: { saving: 'Сохранение снимка…', copying: 'Копирование снимка…', sending: 'Отправка снимка…', canceling: 'Отмена снимка…' },
  errors: {
    saveFailed: 'Не удалось сохранить снимок. Повторите попытку.',
    copyFailed: 'Не удалось скопировать снимок. Повторите попытку.',
    sendFailed: 'Не удалось отправить снимок. Повторите попытку.',
    cancelFailed: 'Не удалось отменить снимок. Повторите попытку.',
  },
} satisfies CaptureMessages
