import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { useFileOpen } from '@/hooks/file-open'
import { isImage } from '@/lib/file'

it('recognizes ICO files as images and opens them in the media viewer', () => {
  const media = vi.fn(), download = vi.fn()
  const items = ref([{ title: 'favicon.ico' }, { title: 'ICON.ICO' }])
  const { openFile } = useFileOpen({ items, getName: item => item.title, openBrowserFile: vi.fn(), viewMedia: media, download })
  for (const item of items.value) {
    expect(isImage(item.title)).toBe(true)
    openFile(item)
    expect(media).toHaveBeenLastCalledWith(items.value, item)
  }
  expect(download).not.toHaveBeenCalled()
})

it('opens document titles with the same preview dispatch as files', () => {
  const text = vi.fn(), browser = vi.fn(), media = vi.fn(), download = vi.fn()
  const items = ref([{ title: 'notes.txt' }, { title: 'paper.pdf' }, { title: 'archive.bin' }])
  const { openFile } = useFileOpen({ items, getName: item => item.title, openTextFile: text, openBrowserFile: browser, viewMedia: media, download })
  items.value.forEach(openFile)
  expect(text).toHaveBeenCalledExactlyOnceWith(items.value[0])
  expect(browser).toHaveBeenCalledExactlyOnceWith(items.value[1])
  expect(download).toHaveBeenCalledExactlyOnceWith(items.value[2])
  expect(media).not.toHaveBeenCalled()
})

it('opens Office documents through the text viewer without a capability gate', () => {
  const items = ref(['report.DOC', 'report.docx', 'sheet.xls', 'sheet.xlsx', 'slides.ppt', 'slides.pptx'].map(title => ({ title })))
  const text = vi.fn(), browser = vi.fn(), media = vi.fn(), download = vi.fn()
  const { openFile } = useFileOpen({ items, getName: item => item.title, openTextFile: text, openBrowserFile: browser, viewMedia: media, download })
  items.value.forEach(openFile)
  expect(text.mock.calls.map(([item]) => item)).toEqual(items.value)
  expect(browser).not.toHaveBeenCalled()
  expect(download).not.toHaveBeenCalled()
})
