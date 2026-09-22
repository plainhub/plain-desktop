import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { GraphQLSyntaxError, parseDocument, parseSchema } from '../../../../src/lib/api/graphql/parser'

const LOCAL_SDL = `
type QueryRoot {
  app: App!
  chatItems(target: String!, offset: Int!, limit: Int!, query: String!): [ChatItem!]!
  count(query: String): Int!
  task: Job
}

enum DeviceType {
  PHONE
  COMPUTER
}

union ChatItemData = ChatImages | ChatText

input NoteInput {
  title: String!
  content: String!
}

interface Node {
  id: ID!
}

type App implements Node {
  id: ID!
  name: String
  size: Long
}

type ChatImages {
  ids: [String!]!
}

type ChatText {
  ids: [String!]!
}

type ChatItem {
  id: ID!
  data: ChatItemData
}

type Job {
  status: String!
}

directive @include(if: Boolean!) on FIELD | FRAGMENT_SPREAD | INLINE_FRAGMENT
`

describe('parseSchema', () => {
  it('parses fields, args, nullability, lists and roots', () => {
    const sdl = parseSchema(LOCAL_SDL)
    expect(sdl.queryRoot).toBe('QueryRoot')
    const root = sdl.types.get('QueryRoot')!
    const chatItems = root.fields.find((f) => f.name === 'chatItems')!
    expect(chatItems.args.map((a) => a.name)).toEqual(['target', 'offset', 'limit', 'query'])
    expect(chatItems.type).toMatchObject({ ofType: 'list', nonNull: true })
    const count = root.fields.find((f) => f.name === 'count')!
    expect(count.args[0].type).toMatchObject({ ofType: 'named', name: 'String', nonNull: false })
    expect(count.type).toMatchObject({ ofType: 'named', name: 'Int', nonNull: true })
  })

  it('parses enums, unions, inputs, interfaces, implements and directives', () => {
    const sdl = parseSchema(LOCAL_SDL)
    expect(sdl.types.get('DeviceType')!.enumValues).toEqual(['PHONE', 'COMPUTER'])
    expect(sdl.types.get('ChatItemData')!.unionMembers).toEqual(['ChatImages', 'ChatText'])
    expect(sdl.types.get('NoteInput')!.fields.map((f) => f.name)).toEqual(['title', 'content'])
    expect(sdl.types.get('App')!.interfaces).toEqual(['Node'])
    expect(sdl.types.get('App')!.fields.find((f) => f.name === 'name')!.type.nonNull).toBe(false)
  })

  it('parses the real local SDL artifact without a fixed type budget', () => {
    const sdlText = readFileSync(join(process.cwd(), 'src/lib/api/graphql/local-schema.graphql'), 'utf8')
    const sdl = parseSchema(sdlText)
    expect(sdl.types.size).toBeGreaterThan(30)
    expect(sdl.types.has('QueryRoot')).toBe(true)
  })

  it('reports the line of a syntax error', () => {
    expect(() => parseSchema('type Broken {\n  field: !!\n}')).toThrow(GraphQLSyntaxError)
    try {
      parseSchema('type Broken {\n  field: !!\n}')
    } catch (e) {
      expect((e as GraphQLSyntaxError).line).toBe(2)
    }
  })
})

describe('parseDocument', () => {
  it('parses operations with variables, aliases, args and fragments', () => {
    const doc = parseDocument(`
      query sms($offset: Int!, $query: String!) {
        items: sms(offset: $offset, limit: 10, query: $query) {
          ...SmsFragment
        }
        smsCount(query: $query)
      }
      fragment SmsFragment on Sms {
        id
        body
      }
    `)
    const op = doc.operations[0]
    expect(op.name).toBe('sms')
    expect(op.variables.map((v) => v.name)).toEqual(['offset', 'query'])
    const field = op.selections[0]
    expect(field).toMatchObject({ kind: 'field', alias: 'items', name: 'sms' })
    if (field.kind === 'field') {
      expect(field.args.map((a) => a.name)).toEqual(['offset', 'limit', 'query'])
    }
    expect(doc.fragments[0]).toMatchObject({ name: 'SmsFragment', typeCondition: 'Sms' })
  })

  it('parses inline fragments and unions selections', () => {
    const doc = parseDocument(`
      query {
        chatItem(id: "x") {
          data {
            ... on ChatText {
              ids
            }
          }
        }
      }
    `)
    const chatItem = doc.operations[0].selections[0]
    if (chatItem.kind !== 'field') throw new Error('expected field')
    const data = chatItem.selections[0]
    expect(data).toMatchObject({ kind: 'field', name: 'data' })
    const inline = data.kind === 'field' ? data.selections[0] : undefined
    expect(inline).toMatchObject({ kind: 'inline', typeCondition: 'ChatText' })
  })

  it('parses list and object argument values', () => {
    const doc = parseDocument(`
      mutation {
        addBookmarks(urls: ["a", "b"], groupId: "g", meta: { depth: 1, flags: [true, false] })
      }
    `)
    const field = doc.operations[0].selections[0]
    if (field.kind !== 'field') throw new Error('expected field')
    expect(field.args.map((a) => a.value.kind)).toEqual(['list', 'string', 'object'])
  })
})
