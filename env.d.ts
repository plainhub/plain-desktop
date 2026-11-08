/// <reference types="vite/client" />
declare const __IS_TAURI__: boolean
declare const __APP_VERSION__: string
declare module '~icons/*' {
  import type { FunctionalComponent, SVGAttributes } from 'vue'
  const component: FunctionalComponent<SVGAttributes>
  export default component
}
declare module '*.svg?component' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent
  export default component
}
interface ImportMetaEnv {
  readonly VITE_APP_API_HOST: string
}
interface ImportMeta {
  readonly env: ImportMetaEnv
}

declare module 'markdown-it'

interface Navigator {
  readonly userAgentData?: {
    readonly brands: Array<{ brand: string; version: string }>
    readonly mobile: boolean
    readonly platform: string
    getHighEntropyValues(hints: string[]): Promise<Record<string, any>>
  }
}

interface Window {
  /** DevTools toggle: `__PLAIN_LOG__ = true` enables transport logging. */
  __PLAIN_LOG__?: boolean
}
