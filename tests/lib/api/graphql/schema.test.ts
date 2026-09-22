import { describe, expect, it } from 'vitest'
import { parseDocument, parseSchema } from '../../../../src/lib/api/graphql/parser'
import { mergeSchemas, validateDocument } from '../../../../src/lib/api/graphql/schema'

const LOCAL = `
type QueryRoot {
  app: App!
  smsCount(query: String): Int!
}

type MutationRoot {
  deleteBookmarks(ids: [ID!]!): ActionResult!
}

type ActionResult {
  affectedCount: Int!
}

type App {
  name: String!
}

type Mount {
  id: String!
  diskId: String!
}
`

const EXTENSION = `
schema {
  query: ExtensionQuery
  mutation: ExtensionMutation
}

type ExtensionQuery {
  sms(offset: Int!, limit: Int!, query: String!): [Sms!]!
}

type ExtensionMutation {
  sendSms(number: String!): Boolean!
}

type Sms {
  id: ID!
  body: String!
}

type Mount {
  label: String
  partitionNum: Int
}
`

function union() {
  const { schema, errors } = mergeSchemas(parseSchema(LOCAL), parseSchema(EXTENSION))
  return { schema, errors }
}

describe('mergeSchemas', () => {
  it('unions roots and types field-by-field', () => {
    const { schema, errors } = union()
    expect(errors).toEqual([])
    const root = schema.types.get('QueryRoot')!
    expect(root.fields.map((f) => f.name)).toEqual(['app', 'smsCount', 'sms'])
    const mount = schema.types.get('Mount')!
    expect(mount.fields.map((f) => f.name)).toEqual(['id', 'diskId', 'label', 'partitionNum'])
  })

  it('rejects a field defined by both schemas', () => {
    const base = parseSchema(LOCAL)
    const dupeRoot = mergeSchemas(base, parseSchema('type QueryRoot { smsCount(query: String): Int! }'))
    expect(dupeRoot.errors.some((e) => e.message.includes("field 'QueryRoot.smsCount' is defined by both"))).toBe(true)
    const dupeType = mergeSchemas(base, parseSchema('type App { name: String! }'))
    expect(dupeType.errors.some((e) => e.message.includes("field 'App.name' is defined by both"))).toBe(true)
  })
})

describe('validateDocument', () => {
  it('accepts a document that conforms to the union schema', () => {
    const { schema } = union()
    const doc = parseDocument(`
      query sms($offset: Int!, $limit: Int!, $query: String!) {
        sms(offset: $offset, limit: $limit, query: $query) {
          id
          body
        }
        smsCount(query: $query)
      }
    `)
    expect(validateDocument(doc, schema, 'ok')).toEqual([])
  })

  it('flags unknown fields, unknown arguments and undeclared variables', () => {
    const { schema } = union()
    const doc = parseDocument(`
      query {
        sms(offset: 0, limit: 1, query: "", bogus: 1) {
          nope
        }
      }
    `)
    const errors = validateDocument(doc, schema, 'bad')
    expect(errors.some((e) => e.message.includes("unknown argument 'bogus'"))).toBe(true)
    expect(errors.some((e) => e.message.includes("unknown field 'nope'"))).toBe(true)
  })

  it('flags selections on scalars and empty selection sets', () => {
    const { schema } = union()
    const scalar = validateDocument(parseDocument('query { smsCount(query: "") { x } }'), schema, 's')
    expect(scalar.some((e) => e.message.includes('cannot have a selection set'))).toBe(true)
    const empty = validateDocument(parseDocument('query { app { name } sms(offset: 0, limit: 1, query: "") { } }'), schema, 'e')
    expect(empty.some((e) => e.message.includes('selection set on \'Sms\' is empty'))).toBe(true)
  })

  it('flags unknown fragments and unknown variable types', () => {
    const { schema } = union()
    const frag = validateDocument(parseDocument('query { sms(offset: 0, limit: 1, query: "") { ...Missing } }'), schema, 'f')
    expect(frag.some((e) => e.message.includes("unknown fragment 'Missing'"))).toBe(true)
    const varType = validateDocument(parseDocument('query q($n: NotAType!) { smsCount(query: "") }'), schema, 'v')
    expect(varType.some((e) => e.message.includes("unknown type 'NotAType'"))).toBe(true)
  })

  it('flags plain fields on union types', () => {
    const { schema } = union()
    const local = parseSchema('type QueryRoot2 { d: U }')
    void local
    const merged = mergeSchemas(schema, parseSchema('type QueryRoot { d: U }\nunion U = A | B\ntype A { x: String }\ntype B { y: String }'))
    expect(merged.errors).toEqual([])
    const errors = validateDocument(parseDocument('query { d { x } }'), merged.schema, 'u')
    expect(errors.some((e) => e.message.includes("only allows __typename"))).toBe(true)
  })
})
