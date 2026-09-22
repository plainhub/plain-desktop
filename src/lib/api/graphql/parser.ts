export interface Tok {
  kind: 'name' | 'int' | 'float' | 'string' | 'punct' | 'eof'
  value: string
  line: number
}

const PUNCT = '{}():=![]|$@&'

export class GraphQLSyntaxError extends Error {
  readonly line: number

  constructor(
    message: string,
    line: number,
  ) {
    super(`line ${line}: ${message}`)
    this.name = 'GraphQLSyntaxError'
    this.line = line
  }
}

class Lexer {
  private i = 0
  private line = 1
  private readonly src: string

  constructor(src: string) {
    this.src = src
  }

  private skipIgnored() {
    while (this.i < this.src.length) {
      const c = this.src[this.i]
      if (c === '#') {
        while (this.i < this.src.length && this.src[this.i] !== '\n') this.i++
      } else if (c === '\n') {
        this.line++
        this.i++
      } else if (c === ',' || /\s/.test(c)) {
        this.i++
      } else {
        break
      }
    }
  }

  next(): Tok {
    this.skipIgnored()
    if (this.i >= this.src.length) return { kind: 'eof', value: '', line: this.line }
    const line = this.line
    const c = this.src[this.i]
    if (c === '.' && this.src.slice(this.i, this.i + 3) === '...') {
      this.i += 3
      return { kind: 'punct', value: '...', line }
    }
    if (PUNCT.includes(c)) {
      this.i++
      return { kind: 'punct', value: c, line }
    }
    if (c === '"') {
      if (this.src.slice(this.i, this.i + 3) === '"""') {
        const end = this.src.indexOf('"""', this.i + 3)
        if (end < 0) throw new GraphQLSyntaxError('unterminated block string', line)
        const raw = this.src.slice(this.i + 3, end)
        this.line += (raw.match(/\n/g) ?? []).length
        this.i = end + 3
        return { kind: 'string', value: raw, line }
      }
      this.i++
      let out = ''
      while (this.i < this.src.length && this.src[this.i] !== '"') {
        if (this.src[this.i] === '\\') {
          out += unescapeChar(this.src[this.i + 1])
          this.i += 2
        } else {
          if (this.src[this.i] === '\n') throw new GraphQLSyntaxError('unterminated string', line)
          out += this.src[this.i]
          this.i++
        }
      }
      if (this.i >= this.src.length) throw new GraphQLSyntaxError('unterminated string', line)
      this.i++
      return { kind: 'string', value: out, line }
    }
    if (c === '-' || (c >= '0' && c <= '9')) {
      let j = this.i + 1
      let isFloat = false
      while (j < this.src.length && /[0-9.eE+-]/.test(this.src[j])) {
        if (/[.eE]/.test(this.src[j])) isFloat = true
        j++
      }
      const value = this.src.slice(this.i, j)
      this.i = j
      return { kind: isFloat ? 'float' : 'int', value, line }
    }
    if (/[A-Za-z_]/.test(c)) {
      let j = this.i + 1
      while (j < this.src.length && /[A-Za-z0-9_]/.test(this.src[j])) j++
      const value = this.src.slice(this.i, j)
      this.i = j
      return { kind: 'name', value, line }
    }
    throw new GraphQLSyntaxError(`unexpected character ${JSON.stringify(c)}`, line)
  }
}

function unescapeChar(c: string | undefined): string {
  switch (c) {
    case 'n':
      return '\n'
    case 't':
      return '\t'
    case 'r':
      return '\r'
    case '"':
      return '"'
    case '\\':
      return '\\'
    case '/':
      return '/'
    default:
      return c ?? ''
  }
}

export type TypeRef =
  | { ofType: 'named'; name: string; nonNull: boolean }
  | { ofType: 'list'; inner: TypeRef; nonNull: boolean }

export interface ArgDef {
  name: string
  type: TypeRef
}

export interface FieldDef {
  name: string
  args: ArgDef[]
  type: TypeRef
}

export type TypeDefKind = 'object' | 'interface' | 'union' | 'enum' | 'input' | 'scalar'

export interface TypeDef {
  kind: TypeDefKind
  name: string
  fields: FieldDef[]
  interfaces: string[]
  unionMembers: string[]
  enumValues: string[]
}

export interface SchemaIR {
  types: Map<string, TypeDef>
  queryRoot: string
  mutationRoot: string
  subscriptionRoot: string
}

export interface VarDef {
  name: string
  type: TypeRef
}

export type Selection =
  | { kind: 'field'; alias: string | undefined; name: string; args: Argument[]; selections: Selection[] }
  | { kind: 'spread'; name: string }
  | { kind: 'inline'; typeCondition: string | undefined; selections: Selection[] }

export interface Argument {
  name: string
  value: ValueNode
}

export type ValueNode =
  | { kind: 'variable'; name: string }
  | { kind: 'int' | 'float'; value: string }
  | { kind: 'string'; value: string }
  | { kind: 'boolean'; value: string }
  | { kind: 'null' }
  | { kind: 'enum'; value: string }
  | { kind: 'list'; values: ValueNode[] }
  | { kind: 'object'; fields: Array<{ name: string; value: ValueNode }> }

export interface OperationDef {
  type: 'query' | 'mutation' | 'subscription'
  name: string | undefined
  variables: VarDef[]
  selections: Selection[]
}

export interface FragmentDef {
  name: string
  typeCondition: string
  selections: Selection[]
}

export interface DocumentIR {
  operations: OperationDef[]
  fragments: FragmentDef[]
}

function describe(t: Tok): string {
  if (t.kind === 'eof') return 'end of input'
  return t.kind === 'string' ? `"${t.value}"` : `'${t.value}'`
}

export class Parser {
  private tok: Tok

  constructor(src: string) {
    const lexer = new Lexer(src)
    this.tok = lexer.next()
    this.lex = () => lexer.next()
  }

  private readonly lex: () => Tok

  peek(): Tok {
    return this.tok
  }

  take(): Tok {
    const t = this.tok
    this.tok = this.lex()
    return t
  }

  expectPunct(value: string): Tok {
    if (this.tok.kind === 'punct' && this.tok.value === value) return this.take()
    throw new GraphQLSyntaxError(`expected '${value}', got ${describe(this.tok)}`, this.tok.line)
  }

  expectName(value?: string): Tok {
    if (this.tok.kind === 'name' && (value === undefined || this.tok.value === value)) return this.take()
    throw new GraphQLSyntaxError(`expected ${value ?? 'a name'}, got ${describe(this.tok)}`, this.tok.line)
  }

  tryPunct(value: string): boolean {
    if (this.tok.kind === 'punct' && this.tok.value === value) {
      this.take()
      return true
    }
    return false
  }

  tryTakeString(): string | undefined {
    if (this.tok.kind === 'string') return this.take().value
    return undefined
  }

  isPunct(value: string): boolean {
    return this.tok.kind === 'punct' && this.tok.value === value
  }

  isName(value: string): boolean {
    return this.tok.kind === 'name' && this.tok.value === value
  }

  parseTypeRef(): TypeRef {
    let ref: TypeRef
    if (this.tryPunct('[')) {
      const inner = this.parseTypeRef()
      this.expectPunct(']')
      ref = { ofType: 'list', inner, nonNull: false }
    } else {
      ref = { ofType: 'named', name: this.expectName().value, nonNull: false }
    }
    if (this.tryPunct('!')) ref.nonNull = true
    return ref
  }

  parseValue(): ValueNode {
    const t = this.peek()
    if (t.kind === 'punct' && t.value === '$') {
      this.take()
      return { kind: 'variable', name: this.expectName().value }
    }
    if (t.kind === 'int' || t.kind === 'float') {
      this.take()
      return { kind: t.kind, value: t.value }
    }
    if (t.kind === 'string') {
      this.take()
      return { kind: 'string', value: t.value }
    }
    if (t.kind === 'name') {
      this.take()
      if (t.value === 'true' || t.value === 'false') return { kind: 'boolean', value: t.value }
      if (t.value === 'null') return { kind: 'null' }
      return { kind: 'enum', value: t.value }
    }
    if (this.isPunct('[')) {
      this.take()
      const values: ValueNode[] = []
      while (!this.isPunct(']')) values.push(this.parseValue())
      this.expectPunct(']')
      return { kind: 'list', values }
    }
    if (this.isPunct('{')) {
      this.take()
      const fields: Array<{ name: string; value: ValueNode }> = []
      while (!this.isPunct('}')) {
        const name = this.expectName().value
        this.expectPunct(':')
        fields.push({ name, value: this.parseValue() })
      }
      this.expectPunct('}')
      return { kind: 'object', fields }
    }
    throw new GraphQLSyntaxError(`expected a value, got ${describe(this.tok)}`, this.tok.line)
  }

  parseSelectionSet(): Selection[] {
    this.expectPunct('{')
    const selections: Selection[] = []
    while (!this.tryPunct('}')) {
      if (this.isPunct('...')) {
        this.take()
        if (this.isName('on')) {
          this.take()
          const typeCondition = this.expectName().value
          this.skipDirectives()
          selections.push({ kind: 'inline', typeCondition, selections: this.parseSelectionSet() })
        } else if (this.isPunct('{')) {
          this.skipDirectives()
          selections.push({ kind: 'inline', typeCondition: undefined, selections: this.parseSelectionSet() })
        } else {
          this.skipDirectives()
          selections.push({ kind: 'spread', name: this.expectName().value })
        }
      } else {
        const first = this.expectName().value
        let alias: string | undefined
        let name = first
        if (this.tryPunct(':')) {
          alias = first
          name = this.expectName().value
        }
        const args = this.parseArguments()
        this.skipDirectives()
        const hasSet = this.isPunct('{')
        selections.push({
          kind: 'field',
          alias,
          name,
          args,
          selections: hasSet ? this.parseSelectionSet() : [],
        })
      }
    }
    return selections
  }

  parseArguments(): Argument[] {
    const args: Argument[] = []
    if (!this.tryPunct('(')) return args
    while (!this.tryPunct(')')) {
      const name = this.expectName().value
      this.expectPunct(':')
      args.push({ name, value: this.parseValue() })
      if (this.isPunct('=')) {
        this.take()
        this.parseValue()
      }
    }
    return args
  }

  parseArgDefs(): ArgDef[] {
    const args: ArgDef[] = []
    if (!this.tryPunct('(')) return args
    while (!this.tryPunct(')')) {
      this.tryTakeString()
      const name = this.expectName().value
      this.expectPunct(':')
      const type = this.parseTypeRef()
      if (this.isPunct('=')) {
        this.take()
        this.parseValue()
      }
      this.skipDirectives()
      args.push({ name, type })
    }
    return args
  }

  skipDirectives() {
    while (this.isPunct('@')) {
      this.take()
      this.expectName()
      if (this.isPunct('(')) {
        this.take()
        while (!this.tryPunct(')')) {
          this.expectName()
          this.expectPunct(':')
          this.parseValue()
          if (this.isPunct('=')) {
            this.take()
            this.parseValue()
          }
        }
      }
    }
  }

  parseDocument(): DocumentIR {
    const operations: OperationDef[] = []
    const fragments: FragmentDef[] = []
    for (;;) {
      const t = this.peek()
      if (t.kind === 'eof') break
      if (t.kind === 'name' && t.value === 'fragment') {
        this.take()
        const name = this.expectName().value
        this.expectName('on')
        const typeCondition = this.expectName().value
        this.skipDirectives()
        fragments.push({ name, typeCondition, selections: this.parseSelectionSet() })
      } else if (t.kind === 'name' && ['query', 'mutation', 'subscription'].includes(t.value)) {
        this.take()
        const type = t.value as OperationDef['type']
        let name: string | undefined
        if (this.tok.kind === 'name') name = this.take().value
        const variables: VarDef[] = []
        if (this.tryPunct('(')) {
          while (!this.tryPunct(')')) {
            this.expectPunct('$')
            const varName = this.expectName().value
            this.expectPunct(':')
            const varType = this.parseTypeRef()
            if (this.isPunct('=')) {
              this.take()
              this.parseValue()
            }
            variables.push({ name: varName, type: varType })
          }
        }
        this.skipDirectives()
        operations.push({ type, name, variables, selections: this.parseSelectionSet() })
      } else {
        throw new GraphQLSyntaxError(`expected an operation or fragment, got ${describe(this.tok)}`, this.tok.line)
      }
    }
    return { operations, fragments }
  }
}

export function parseDocument(src: string): DocumentIR {
  return new Parser(src).parseDocument()
}

export function parseSchema(src: string): SchemaIR {
  const p = new Parser(src)
  const types = new Map<string, TypeDef>()
  let queryRoot = 'Query'
  let mutationRoot = 'Mutation'
  let subscriptionRoot = 'Subscription'
  let hasSchemaBlock = false
  for (;;) {
    p.tryTakeString()
    if (p.peek().kind === 'eof') break
    const kw = p.expectName().value
    if (kw === 'schema') {
      hasSchemaBlock = true
      p.expectPunct('{')
      while (!p.tryPunct('}')) {
        const role = p.expectName().value
        p.expectPunct(':')
        const root = p.expectName().value
        if (role === 'query') queryRoot = root
        else if (role === 'mutation') mutationRoot = root
        else if (role === 'subscription') subscriptionRoot = root
      }
      continue
    }
    if (kw === 'directive') {
      p.expectPunct('@')
      p.expectName()
      if (p.isPunct('(')) {
        p.take()
        while (!p.tryPunct(')')) {
          p.expectName()
          p.expectPunct(':')
          p.parseTypeRef()
          if (p.isPunct('=')) {
            p.take()
            p.parseValue()
          }
        }
      }
      p.expectName('on')
      for (;;) {
        p.expectName()
        if (!p.tryPunct('|')) break
      }
      continue
    }
    if (kw === 'extend') throw new GraphQLSyntaxError('type extensions are not supported', p.peek().line)
    let kind: TypeDefKind
    if (kw === 'type') kind = 'object'
    else if (kw === 'interface') kind = 'interface'
    else if (kw === 'union') kind = 'union'
    else if (kw === 'enum') kind = 'enum'
    else if (kw === 'input') kind = 'input'
    else if (kw === 'scalar') kind = 'scalar'
    else throw new GraphQLSyntaxError(`expected a type definition, got '${kw}'`, p.peek().line)
    const name = p.expectName().value
    const def: TypeDef = { kind, name, fields: [], interfaces: [], unionMembers: [], enumValues: [] }
    if ((kind === 'object' || kind === 'interface') && p.isName('implements')) {
      p.take()
      for (;;) {
        if (p.tryPunct('&')) continue
        def.interfaces.push(p.expectName().value)
        if (!p.isPunct('&')) break
      }
    }
    if (kind === 'scalar') {
      p.skipDirectives()
      types.set(name, def)
      continue
    }
    if (kind === 'union') {
      p.expectPunct('=')
      for (;;) {
        def.unionMembers.push(p.expectName().value)
        if (!p.tryPunct('|')) break
      }
      types.set(name, def)
      continue
    }
    if (kind === 'enum') {
      p.expectPunct('{')
      while (!p.tryPunct('}')) {
        p.tryTakeString()
        def.enumValues.push(p.expectName().value)
        p.skipDirectives()
      }
      types.set(name, def)
      continue
    }
    p.expectPunct('{')
    while (!p.tryPunct('}')) {
      p.tryTakeString()
      const fieldName = p.expectName().value
      const args = kind === 'input' ? [] : p.parseArgDefs()
      p.expectPunct(':')
      const fieldType = p.parseTypeRef()
      if (p.isPunct('=')) {
        p.take()
        p.parseValue()
      }
      p.skipDirectives()
      def.fields.push({ name: fieldName, args, type: fieldType })
    }
    types.set(name, def)
  }
  if (!hasSchemaBlock && !types.has('Query') && types.has('QueryRoot')) queryRoot = 'QueryRoot'
  if (!hasSchemaBlock && !types.has('Mutation') && types.has('MutationRoot')) mutationRoot = 'MutationRoot'
  return { types, queryRoot, mutationRoot, subscriptionRoot }
}
