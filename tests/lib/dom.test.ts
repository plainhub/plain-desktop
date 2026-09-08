import { afterEach, describe, expect, it, vi } from 'vitest'
import { isEditableTarget, selectedTextWithin } from '@/lib/dom'

function fakeSelection(text: string, container: Node | null, rangeCount = 1) {
  return {
    isCollapsed: !text,
    rangeCount,
    getRangeAt: () => ({ commonAncestorContainer: container }),
    toString: () => text,
  } as unknown as Selection
}

function buildMessageDom() {
  const root = document.createElement('div')
  const pre = document.createElement('pre')
  pre.textContent = 'hello world'
  root.append(pre)
  return { root, textNode: pre.firstChild as Text }
}

const spies: ReturnType<typeof vi.spyOn>[] = []

function stubSelection(text: string, container: Node | null, rangeCount = 1) {
  const spy = vi.spyOn(window, 'getSelection').mockReturnValue(fakeSelection(text, container, rangeCount))
  spies.push(spy)
}

afterEach(() => {
  for (const spy of spies.splice(0)) spy.mockRestore()
})

describe('selectedTextWithin', () => {
  it('returns the selected text when the selection is inside the element', () => {
    const { root, textNode } = buildMessageDom()
    stubSelection('world', textNode)
    expect(selectedTextWithin(root)).toBe('world')
  })

  it('returns empty string when the selection is outside the element', () => {
    const { root, textNode } = buildMessageDom()
    const other = document.createElement('div')
    stubSelection('other', other)
    expect(selectedTextWithin(root)).toBe('')
  })

  it('returns empty string when the selection is collapsed', () => {
    const { root, textNode } = buildMessageDom()
    stubSelection('', textNode)
    expect(selectedTextWithin(root)).toBe('')
  })

  it('returns empty string when there is no range', () => {
    const { root, textNode } = buildMessageDom()
    stubSelection('world', textNode, 0)
    expect(selectedTextWithin(root)).toBe('')
  })

  it('returns empty string when the element is null', () => {
    stubSelection('world', document.body)
    expect(selectedTextWithin(null)).toBe('')
  })
})

describe('isEditableTarget', () => {
  it('detects input and textarea targets', () => {
    expect(isEditableTarget(document.createElement('input'))).toBe(true)
    expect(isEditableTarget(document.createElement('textarea'))).toBe(true)
    expect(isEditableTarget(document.createElement('div'))).toBe(false)
  })

  it('returns false for null targets', () => {
    expect(isEditableTarget(null)).toBe(false)
  })
})
