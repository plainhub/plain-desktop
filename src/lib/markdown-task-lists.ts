interface TaskToken {
  type: string
  level: number
  content: string
  children: TaskToken[] | null
  attrSet(name: string, value: string): void
}

interface TaskListState {
  tokens: TaskToken[]
  Token: new (type: string, tag: string, nesting: number) => TaskToken
}

export function markdownTaskLists(md: {
  core: { ruler: { after(after: string, name: string, rule: (state: TaskListState) => void): void } }
}) {
  md.core.ruler.after('inline', 'task-lists', (state) => {
    for (let index = 2; index < state.tokens.length; index++) {
      const token = state.tokens[index]!
      const item = state.tokens[index - 2]!
      if (token.type !== 'inline' || state.tokens[index - 1]!.type !== 'paragraph_open' || item.type !== 'list_item_open') continue
      const marker = /^\[([ xX])\] /.exec(token.content)
      if (!marker) continue
      const children = token.children!
      children[0]!.content = children[0]!.content.slice(3)
      token.content = token.content.slice(3)
      const checkbox = new state.Token('html_inline', '', 0)
      checkbox.content = `<input class="task-list-item-checkbox"${marker[1] === ' ' ? '' : ' checked=""'} type="checkbox">`
      children.unshift(checkbox)
      item.attrSet('class', 'task-list-item enabled')
      for (let parent = index - 3; parent >= 0; parent--) {
        if (state.tokens[parent]!.level === item.level - 1) {
          state.tokens[parent]!.attrSet('class', 'contains-task-list')
          break
        }
      }
    }
  })
}
