<template>
  <div class="md-preview-page">
    <header class="md-preview-header">
      <h1>Markdown Preview</h1>
    </header>
    <main class="md-preview-main">
      <div class="md-container" v-html="rendered"></div>
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import 'katex/dist/katex.min.css'
import MarkdownIt from 'markdown-it'
import subscript from 'markdown-it-sub'
import superscript from 'markdown-it-sup'
import footnote from 'markdown-it-footnote'
import deflist from 'markdown-it-deflist'
import abbreviation from 'markdown-it-abbr'
import insert from 'markdown-it-ins'
import mark from 'markdown-it-mark'
import texmath from 'markdown-it-texmath'
import katex from 'katex'
import tasklists from 'markdown-it-task-lists'

const md = new MarkdownIt({
  html: true,
  xhtmlOut: true,
  breaks: true,
  linkify: true,
  typographer: true,
})
  .use(subscript)
  .use(superscript)
  .use(footnote)
  .use(deflist)
  .use(abbreviation)
  .use(insert)
  .use(mark)
  .use(texmath, { engine: katex, delimiters: 'dollars', katexOptions: { output: 'html' } })
  .use(tasklists, { enabled: true })

const SAMPLE_MARKDOWN = `# Heading 1

## Heading 2

### Heading 3

#### Heading 4

This is a regular paragraph with some content. It demonstrates the unified markdown reading theme. The quick brown fox jumps over the lazy dog. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

---

## Text Formatting

You can use **bold text**, *italic text*, and ***both***. There's also ~~strikethrough~~ and \`inline code\` formatting. superscript: x^2^, subscript: H~2~O, ++inserted text++, and ==marked text==.

## Lists

### Unordered List
- First item
- Second item
  - Nested item
  - Another nested item
- Third item

### Ordered List
1. First step
2. Second step
3. Third step

### Task List
- [x] Completed task
- [x] Another completed task
- [ ] Pending task

## Code Blocks

\`\`\`kotlin
fun main() {
    val greeting = "Hello, Markdown!"
    println(greeting)
}
\`\`\`

\`\`\`javascript
function fibonacci(n) {
    if (n <= 1) return n;
    return fibonacci(n - 1) + fibonacci(n - 2);
}
\`\`\`

## Blockquotes

> "This is a blockquote. It's often used to highlight important information or quotes from other sources."
> - Source attribution

> Nested blockquote with multiple lines.
> Each line starts with a > character.

## Tables

| Feature | Description |
|---------|-------------|
| Headings | Clear hierarchy with proper spacing |
| Code | Dark background with monospace font |
| Quote | Muted text with left accent border |
| Table | Clean borders, comfortable rows with rounded corners |

## Links

[Visit PlainApp](https://plainapp.app) for more information.

## Images

![Placeholder Image](https://picsum.photos/600/300)

## Math

Inline math: $E = mc^2$

Block math:
$$
\\int_0^1 x^2 dx = \\frac{1}{3}
$$

## Definition List

Term 1
: Definition for term 1

Term 2
: Definition for term 2

## Footnotes

Here is a footnote reference[^1].

[^1]: This is the footnote content.
`

const rendered = computed(() => md.render(SAMPLE_MARKDOWN))
</script>

<style lang="scss" scoped>
.md-preview-page {
  max-width: 820px;
  margin: 0 auto;
  padding: 24px 16px 64px;
}

.md-preview-header {
  position: sticky;
  top: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 24px;
  padding: 12px 0;
  background: var(--md-sys-color-surface);
  border-bottom: 1px solid var(--md-sys-color-outline-variant);

  h1 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    color: var(--md-sys-color-on-surface);
  }
}
</style>
