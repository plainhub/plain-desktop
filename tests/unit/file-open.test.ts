import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { useFileOpen } from '@/hooks/file-open'

it('opens document titles with the same preview dispatch as files', () => {
  const text = vi.fn(), browser = vi.fn(), media = vi.fn(), download = vi.fn()
  const items = ref([{ title: 'notes.txt' }, { title: 'paper.pdf' }, { title: 'sheet.xlsx' }])
  const { openFile } = useFileOpen({ items, getName: item => item.title, openTextFile: text, openBrowserFile: browser, viewMedia: media, download })
  items.value.forEach(openFile)
  expect(text).toHaveBeenCalledExactlyOnceWith(items.value[0])
  expect(browser).toHaveBeenCalledExactlyOnceWith(items.value[1])
  expect(download).toHaveBeenCalledExactlyOnceWith(items.value[2])
  expect(media).not.toHaveBeenCalled()
})
