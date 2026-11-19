import { state } from './state'
import { configuration } from './config'
import { closeById, getCurrentModal } from '../methods'

export default function initialize() {
  state.initialized = true

  document.addEventListener('keyup', (e) => {
    // Closing the last modal window when user pressed Escape
    if (configuration.escClose && (e.key === 'Escape' || e.code === 'Escape')) {
      const modal = getCurrentModal()
      if (!modal) return
      if (modal.instance?.onEscape) void modal.instance.onEscape()
      else void closeById(modal.id, { esc: true })
    }
  })
}
