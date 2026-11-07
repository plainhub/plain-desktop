import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import Icons from 'unplugin-icons/vite'
import { svgComponents } from './build-support/svg-components'

export default defineConfig({
  plugins: [vue({ template: { compilerOptions: { isCustomElement: tag => tag.startsWith('i-material-symbols:') || tag.startsWith('i-lucide:') } } }), svgComponents(), Icons()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  test: { environment: 'node', include: ['tests/unit/**/*.test.ts'] },
})
