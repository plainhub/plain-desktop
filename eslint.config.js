import js from '@eslint/js'
import pluginVue from 'eslint-plugin-vue'
import tseslint from '@typescript-eslint/eslint-plugin'
import tsparser from '@typescript-eslint/parser'
import vueParser from 'vue-eslint-parser'
import prettier from 'eslint-config-prettier'

export default [
  {
    ignores: [
      'node_modules/**',
      'trash/**',
      'dist/**',
      'build/**',
      '*.min.js',
      'coverage/**',
      '.yarn/**',
      '.yarn/cache/**',
      '.yarn/install-state.gz',
      '.pnp.*',
      'components.d.ts',
      // Tauri (Rust) build artifacts - not part of the JS/TS codebase
      'src-tauri/target/**',
      'src-tauri/gen/**',
      // i18n pipeline intermediates
      'scripts/i18n-todo.json',
      'scripts/i18n-translated.json',
      '.crowdin/**',
    ],
  },
  js.configs.recommended,
  ...pluginVue.configs['flat/recommended'],
  {
    files: ['**/*.{js,vue,ts,tsx}'],
    languageOptions: {
      globals: {
        // Browser globals
        window: 'readonly',
        document: 'readonly',
        console: 'readonly',
        localStorage: 'readonly',
        sessionStorage: 'readonly',
        navigator: 'readonly',
        location: 'readonly',
        fetch: 'readonly',
        URL: 'readonly',
        URLSearchParams: 'readonly',
        FormData: 'readonly',
        Blob: 'readonly',
        File: 'readonly',
        FileReader: 'readonly',
        Image: 'readonly',
        Audio: 'readonly',
        Video: 'readonly',
        HTMLElement: 'readonly',
        HTMLDivElement: 'readonly',
        HTMLInputElement: 'readonly',
        HTMLImageElement: 'readonly',
        Element: 'readonly',
        Node: 'readonly',
        NodeList: 'readonly',
        Event: 'readonly',
        MouseEvent: 'readonly',
        KeyboardEvent: 'readonly',
        TouchEvent: 'readonly',
        DragEvent: 'readonly',
        ClipboardEvent: 'readonly',
        PointerEvent: 'readonly',
        EventTarget: 'readonly',
        XMLHttpRequest: 'readonly',
        AbortController: 'readonly',
        setTimeout: 'readonly',
        clearTimeout: 'readonly',
        setInterval: 'readonly',
        clearInterval: 'readonly',
        requestAnimationFrame: 'readonly',
        cancelAnimationFrame: 'readonly',
        performance: 'readonly',
        crypto: 'readonly',
        btoa: 'readonly',
        atob: 'readonly',
        TextEncoder: 'readonly',
        TextDecoder: 'readonly',
        MutationObserver: 'readonly',
        ResizeObserver: 'readonly',
        IntersectionObserver: 'readonly',
        // Web Workers
        self: 'readonly',
        importScripts: 'readonly',
        // TypeScript types (may be used at runtime)
        DocumentOrShadowRoot: 'readonly',
        CSSStyleSheet: 'readonly',
        TimerHandler: 'readonly',
        TouchList: 'readonly',
        ChildNode: 'readonly',
        // Third-party libraries
        sjcl: 'readonly',
      },
      parser: tsparser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module',
        extraFileExtensions: ['.vue'],
      },
    },
    plugins: {
      '@typescript-eslint': tseslint,
    },
    rules: {
      'vue/no-deprecated-slot-attribute': 'off',
      'no-control-regex': 'off',
      'vue/multi-word-component-names': 'off',
      'no-irregular-whitespace': 'off', // Too many false positives in this codebase
      'no-useless-escape': 'off', // Too many false positives in Monaco Editor and other libs
      'no-unused-vars': 'off', // Let TypeScript handle this
      '@typescript-eslint/no-unused-vars': 'off', // Too noisy for this codebase
      'no-undef': 'off', // TypeScript handles this better
      'getter-return': 'off', // TypeScript handles this
      'no-case-declarations': 'off',
      'no-fallthrough': 'off',
      'no-prototype-built-ins': 'off',
    },
  },
  // Vue-specific configuration
  {
    files: ['**/*.vue'],
    languageOptions: {
      parser: vueParser,
      parserOptions: {
        parser: tsparser,
        ecmaVersion: 2022,
        sourceType: 'module',
      },
    },
  },
  // Node.js scripts (run via `node` / `node --experimental-vm-modules`)
  {
    files: ['scripts/**/*.{js,mjs,cjs}'],
    languageOptions: {
      globals: {
        process: 'readonly',
        console: 'readonly',
        __dirname: 'readonly',
        __filename: 'readonly',
        global: 'readonly',
        Buffer: 'readonly',
        module: 'readonly',
        require: 'readonly',
        exports: 'readonly',
      },
    },
  },
  // Test files (Node + Vitest globals)
  {
    files: ['tests/**/*.{js,mjs,cjs,ts,tsx}'],
    languageOptions: {
      globals: {
        process: 'readonly',
        console: 'readonly',
        __dirname: 'readonly',
        __filename: 'readonly',
        global: 'readonly',
        Buffer: 'readonly',
        module: 'readonly',
        require: 'readonly',
        exports: 'readonly',
        describe: 'readonly',
        it: 'readonly',
        test: 'readonly',
        expect: 'readonly',
        beforeAll: 'readonly',
        afterAll: 'readonly',
        beforeEach: 'readonly',
        afterEach: 'readonly',
        vi: 'readonly',
        vitest: 'readonly',
      },
    },
  },
  prettier,
]
