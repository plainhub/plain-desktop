<template>
  <section class="ux-md-editor">
    <h2>MarkdownEditor (Live Preview)</h2>
    <p class="hint">
      光标所在元素显示原始 Markdown 标记，移开后原地渲染；空行输入 <code>/</code> 呼出块插入菜单。
    </p>
    <div class="toolbar">
      <button type="button" @click="toggleDark">{{ isDark ? '☀️ Light' : '🌙 Dark' }}</button>
    </div>
    <div class="editor-frame">
      <markdown-editor v-model="content" placeholder="Write markdown... (input / for blocks)" />
    </div>
  </section>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import MarkdownEditor from '@/components/MarkdownEditor.vue'
import emitter from '@/plugins/eventbus'

const content = ref(`# 产品设计评审

本周与团队同步了 **PlainDesk 2.0** 的整体方向，重点是 *编辑器* 的交互模型。参考资料见 [发布计划](https://example.com)，接口约定在 \`useNoteEdit.ts\`。

## 待办事项

- [x] 整理竞品交互对比
- [ ] 绘制 Live Preview 高保真稿
- [ ] ~~手写富文本解析器~~ 改用 syntaxTree

\`\`\`ts
const view = new EditorView({ state, parent })
\`\`\`

缩进代码：

    indented code line
    second line

![架构图](data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='360'%20height='120'%3E%3Crect%20width='360'%20height='120'%20fill='%233f51b5'/%3E%3Ctext%20x='180'%20y='68'%20font-size='22'%20fill='white'%20text-anchor='middle'%20font-family='sans-serif'%3EPlainDesk%20Architecture%3C/text%3E%3C/svg%3E)

| 指标 | 冷启动 | 滚动 |
| :--- | ---: | :---: |
| 改造前 | 240ms | 12% 掉帧 |
| **工具** | \`vite\` | ~~installed~~ |
| 改造后 | 90ms | 0 掉帧 |

行内公式 $E = mc^2$ 与积分：

$$
\\int_0^\\infty e^{-x} \\, dx = 1
$$

---

---

## Table of Contents

*   [Method 1: PlainApp — no app on the iPhone](#method-1-plainapp-no-app-on-the-iphone)
*   [Method 2: LocalSend — an app on both sides](#method-2-localsend-an-app-on-both-sides)

![Diagram 1](data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20width='360'%20height='80'%3E%3Crect%20width='360'%20height='80'%20fill='%2300897b'/%3E%3Ctext%20x='180'%20y='48'%20font-size='20'%20fill='white'%20text-anchor='middle'%20font-family='sans-serif'%3EDiagram%201%3C/text%3E%3C/svg%3E)

## Method 1: PlainApp — no app on the iPhone

方法一的内容段落。

> 编辑器是笔记应用的主战场。
`)

const isDark = ref(document.documentElement.classList.contains('dark'))

function toggleDark() {
  isDark.value = !isDark.value
  document.documentElement.classList.toggle('dark', isDark.value)
  emitter.emit('color_mode_changed')
}
</script>

<style scoped lang="scss">
.ux-md-editor {
  margin-bottom: 48px;

  h2 {
    margin-bottom: 8px;
  }

  .hint {
    font-size: 13px;
    color: var(--md-sys-color-on-surface-variant);
    margin-bottom: 12px;
  }

  .toolbar {
    margin-bottom: 12px;

    button {
      padding: 6px 14px;
      border-radius: 8px;
      border: 1px solid var(--md-sys-color-outline-variant);
      background: var(--md-sys-color-surface-container-low);
      color: var(--md-sys-color-on-surface);
      cursor: pointer;
    }
  }

  .editor-frame {
    height: 520px;
    border: 1px solid var(--md-sys-color-outline-variant);
    border-radius: 14px;
    overflow: hidden;
    background: var(--md-sys-color-surface);
  }
}
</style>
