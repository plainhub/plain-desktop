import Toaster from './PToaster.vue'
import { render, h } from 'vue'

export default (message: string, type = '') => {
  render(h(Toaster, { message, type }), document.createElement('div'))
}

export const toastWithAction = (message: string, actionLabel: string, onAction: () => void, type = '') => {
  render(
    h(Toaster, {
      message,
      type,
      duration: 5000,
      actionLabel,
      onAction,
    }),
    document.createElement('div'),
  )
}
