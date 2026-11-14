import { invoke } from '@/lib/api/native-invoke'
import { i18n } from '@/plugins/i18n'

interface MenuLabels {
  about: string
  quit: string
  services: string
  hide: string
  hideOthers: string
  showAll: string
  file: string
  newWindow: string
  edit: string
  undo: string
  redo: string
  cut: string
  copy: string
  paste: string
  selectAll: string
  view: string
  toggleDevtools: string
  window: string
  minimize: string
  maximize: string
  fullscreen: string
  closeWindow: string
}

const load = (key: string) => i18n.global.t(key)

export function applyMenuLabels(): void {
  if (!__IS_TAURI__) return
  const labels: MenuLabels = {
    about: `${load('menu.about')} PlainApp`,
    quit: `${load('menu.quit')} PlainApp`,
    services: load('menu.services'),
    hide: `${load('menu.hide')} PlainApp`,
    hideOthers: load('menu.hide_others'),
    showAll: load('menu.show_all'),
    file: load('menu.file'),
    newWindow: load('menu.new_window'),
    edit: load('menu.edit'),
    undo: load('menu.undo'),
    redo: load('menu.redo'),
    cut: load('menu.cut'),
    copy: load('menu.copy'),
    paste: load('menu.paste'),
    selectAll: load('menu.select_all'),
    view: load('menu.view'),
    toggleDevtools: load('menu.toggle_devtools'),
    window: load('menu.window'),
    minimize: load('menu.minimize'),
    maximize: load('menu.maximize'),
    fullscreen: load('menu.fullscreen'),
    closeWindow: load('menu.close_window'),
  }
  invoke('set_menu_locale', { labels }).catch(() => {})
}