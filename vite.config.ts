/// <reference types="vitest" />
import { defineConfig, loadEnv } from 'vite'
import vue from '@vitejs/plugin-vue'
import path from 'path'
import Icons from 'unplugin-icons/vite'
import IconsResolver from 'unplugin-icons/resolver'
import Components from 'unplugin-vue-components/vite'
import svgLoader from 'vite-svg-loader'
import vueJsx from '@vitejs/plugin-vue-jsx'
import { playwright } from '@vitest/browser-playwright'
import { isTauriBuildMode } from './build-support/app-mode'

const INVALID_CHAR_REGEX = /[_\x00-\x1F\x7F<>*#"{}|^[\]`;?:&=+$,]/g
const DRIVE_LETTER_REGEX = /^[a-z]:/i
function sanitizeFileName(name: string): string {
  const match = DRIVE_LETTER_REGEX.exec(name)
  const driveLetter = match ? match[0] : ''

  // A `:` is only allowed as part of a windows drive letter (ex: C:\foo)
  // Otherwise, avoid them because they can refer to NTFS alternate data streams.
  return driveLetter + name.substring(driveLetter.length).replace(INVALID_CHAR_REGEX, '')
}

// https://vitejs.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const apiHost = env.VITE_APP_API_HOST || ''
  const isTauriMode = isTauriBuildMode(mode, process.env.VITE_APP_MODE)

  return {
  css: {
    preprocessorOptions: {
      scss: {
        charset: false,
        api: 'modern-compiler'
      }
    }
  },
  server: {
    host: '0.0.0.0',
    port: 4000,
    // WebCodecs requires cross-origin isolation for the hardware-accelerated
    // VideoDecoder/AudioDecoder paths — without these headers `crossOriginIsolated`
    // is false, `decode()` throws `Decoder error` even though `isConfigSupported`
    // reports supported. `credentialless` is preferred over `require-corp` because
    // it does not block cross-origin subresources (avatars, fonts, etc.).
    headers: {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'credentialless',
    },
    // Dev-only proxy: forwards /fs and /proxyfs to the device server so that
    // fetch() and WebGL textures are same-origin (no CORS / canvas tainting).
    // In production the app is served from the device itself (same-origin).
    ...(apiHost ? {
      proxy: {
        '/fs': { target: `http://${apiHost}`, changeOrigin: true },
        '/proxyfs': { target: `http://${apiHost}`, changeOrigin: true },
      },
    } : {}),
  },
  build: {
    // The `markdown` chunk (katex + markdown-it) intentionally bundles the
    // whole math/markdown stack into one async chunk so it is loaded only
    // when a markdown view opens. It is ~640 kB unminified / ~204 kB gzipped,
    // so raise the default 500 kB warning threshold to fit it without noise.
    chunkSizeWarningLimit: 700,
    // Disable the per-plugin build-time diagnostic — unplugin-icons,
    // unplugin-vue-components, and vite-svg-loader are JS-side plugins by
    // design and dominate wall time on this project. The diagnostic is
    // useful when chasing regressions in native plugins, not here.
    //
    // Also disable the "ineffective dynamic import" check. `main.ts`
    // intentionally dynamic-imports `src/plugins/i18n.ts` to defer module
    // evaluation until `preloadPrefs()` has run (Tauri mode reads locale
    // from the persisted plugin-store, not localStorage). `router.ts`,
    // `useLocaleSwitch.ts`, and `TauriTabBar.vue` statically import the
    // same module for synchronous `i18n.global.t(...)` calls, so the
    // dynamic import can never produce a chunk split — that is a
    // deliberate trade-off, not a bug.
    rolldownOptions: {
      checks: {
        pluginTimings: false,
        ineffectiveDynamicImport: false,
      },
    },
    rollupOptions: {
      output: {
        sanitizeFileName(fileName) {
          return sanitizeFileName(fileName)
        },
        assetFileNames(assetInfo) {
          const extType = assetInfo.name?.split('.')?.at(1) ?? ''
          if (/woff2|woff|ttf/i.test(extType)) {
            return 'assets/[name][extname]'
          }
          return 'assets/[name]-[hash][extname]'
        },
        manualChunks(id) {
          // Merge all locale module files for the same language into one
          // async chunk, so switching languages costs a single network
          // round-trip per locale instead of N tiny requests. The fallback
          // locale (en-US) is eagerly imported at startup, so we leave it
          // in the main bundle to avoid an extra round-trip on cold start.
          const localeMatch = id.match(/[\\/]locales[\\/]([^\\/]+)[\\/]/)
          if (localeMatch && localeMatch[1] !== 'en-US') return `locale-${localeMatch[1]}`
          // Bundle heavy markdown stack into its own chunk so it does not
          // inflate the main entry. Rolldown chunks more aggressively than
          // Rollup by default, so we re-merge the katex + markdown-it
          // graph here to avoid one-chunk-per-import fragmentation.
          if (/[\\/](katex|markdown-it|markdown-it-[^\\/]+|@katex)[\\/]/.test(id)) {
            return 'markdown'
          }
        },
      },
    },
  },
  oxc: { legalComments: 'none' },
  plugins: [
    vue({
      template: {
        compilerOptions: {
          isCustomElement: (tag) => tag.startsWith('md-'),
        },
      },
    }),
    vueJsx(),
    svgLoader(),
    Components({
      resolvers: [IconsResolver()],
      dirs: ['src/components', 'src/views'],
    }),
    Icons(),
  ],
  define: {
    'process.env': {},
    __VUE_I18N_FULL_INSTALL__: true,
    __VUE_I18N_LEGACY_API__: false,
    __INTLIFY_PROD_DEVTOOLS__: false,
    __IS_TAURI__: JSON.stringify(isTauriMode),
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src'),
    },
  },
  test: {
    // Three projects, three environments:
    //
    // - `unit` runs in real Chromium (Playwright) — gives us native
    //   crypto.getRandomValues / Response.text() / localStorage with no
    //   polyfill drift. This is also the same runtime as the Tauri
    //   WebView and the production web build.
    // - `cws` runs in Node — cross-window-store needs module-level state
    //   isolation via `vi.resetModules()`, which only works on the Node
    //   side. Vitest Browser Mode keeps module-level state across the
    //   page side, so those tests cannot simulate two windows without a
    //   Node environment.
    // - `integration` runs in Node — hits live Rust + Android GraphQL
    //   endpoints over HTTPS to verify cross-API schema/behavior
    //   alignment. Gated on `.env.test.local` config; skips cleanly
    //   when endpoints aren't configured. See
    //   tests/integration/README.md.
    //
    // Vitest 4 projects don't inherit root `resolve.alias` or `define`,
    // so each project re-declares them.
    //
    // All projects run in a single thread each, sequentially across
    // projects, so vi.spyOn / vi.resetModules from one project cannot
    // stomp the other's Node-side module cache mid-flight.
    resolve: {
      alias: {
        '@': path.resolve(__dirname, 'src'),
      },
    },
    projects: [
      {
        plugins: [vue()],
        resolve: {
          alias: {
            '@': path.resolve(__dirname, 'src'),
          },
        },
        define: {
          __IS_TAURI__: JSON.stringify(isTauriMode),
        },
        test: {
          name: 'unit',
          include: ['tests/**/*.test.ts'],
          exclude: [
            'tests/lib/cross-window-store.test.ts',
            'tests/integration/**/*.test.ts',
          ],
          browser: {
            enabled: true,
            provider: playwright(),
            headless: true,
            instances: [{ browser: 'chromium' }],
          },
          globals: true,
          setupFiles: ['./tests/setup.ts'],
          coverage: { provider: 'v8', include: ['src/lib/**'] },
          pool: 'threads',
          poolOptions: { threads: { singleThread: true } },
        },
      },
      {
        resolve: {
          alias: {
            '@': path.resolve(__dirname, 'src'),
          },
        },
        define: {
          __IS_TAURI__: JSON.stringify(isTauriMode),
        },
        test: {
          name: 'cws',
          include: ['tests/lib/cross-window-store.test.ts'],
          environment: 'node',
          globals: true,
          setupFiles: ['./tests/setup.cws.ts'],
          pool: 'threads',
          poolOptions: { threads: { singleThread: true } },
        },
      },
      {
        resolve: {
          alias: {
            '@': path.resolve(__dirname, 'src'),
          },
        },
        define: {
          __IS_TAURI__: JSON.stringify(isTauriMode),
        },
        test: {
          name: 'integration',
          include: ['tests/integration/**/*.test.ts'],
          environment: 'node',
          globals: true,
          setupFiles: ['./tests/integration/setup.ts'],
          pool: 'threads',
          poolOptions: { threads: { singleThread: true } },
        },
      },
    ],
  },
}
})
