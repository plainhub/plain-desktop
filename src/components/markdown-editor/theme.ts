import { EditorView } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags as highlightTags } from '@lezer/highlight'

export const baseTheme = EditorView.theme({
  '&': { height: '100%', fontSize: '16px' },
  '.cm-scroller': {
    overflow: 'auto',
    lineHeight: '1.75',
    fontFamily:
      "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', sans-serif",
  },
  '.cm-content': {
    maxWidth: '820px',
    margin: '0 auto',
    padding: '16px 40px 64px',
    caretColor: 'var(--md-sys-color-primary)',
  },
  '.cm-placeholder': { color: 'var(--md-sys-color-on-surface-variant)' },
  '&.cm-focused': { outline: 'none' },
  '.cm-md-h1-line': { paddingTop: '32px', paddingBottom: '12px' },
  '.cm-md-h2-line': { paddingTop: '28px', paddingBottom: '12px' },
  '.cm-md-h3-line': { paddingTop: '24px', paddingBottom: '8px' },
  '.cm-md-h4-line': { paddingTop: '20px', paddingBottom: '6px' },
  '.cm-md-h5-line': { paddingTop: '16px', paddingBottom: '4px' },
  '.cm-md-h6-line': { paddingTop: '16px', paddingBottom: '4px' },
})

export const lightTheme = EditorView.theme({
  '.cm-selectionBackground': { backgroundColor: 'rgba(63, 81, 181, 0.18) !important' },
})

export const darkThemeOverride = EditorView.theme({
  '.cm-selectionBackground': { backgroundColor: 'rgba(190, 194, 255, 0.28) !important' },
})

const mdHighlightStyle = HighlightStyle.define([
  { tag: highlightTags.heading, color: 'var(--md-sys-color-on-surface)', textDecoration: 'none', fontWeight: '600' },
  { tag: highlightTags.link, color: 'var(--md-sys-color-primary)', textDecoration: 'none' },
])

export const mdHighlightExtensions = syntaxHighlighting(mdHighlightStyle)
