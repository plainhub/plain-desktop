import { describe, expect, it } from 'vitest'
import { parseDocument, parseSchema } from '../../../../src/lib/api/graphql/parser'
import { mergeSchemas } from '../../../../src/lib/api/graphql/schema'
import { generateOperationsSource } from '../../../../src/lib/api/graphql/codegen'

const SDL = `
type QueryRoot {
  sms(offset: Int!, limit: Int!, query: String!): [Sms!]!
  smsCount(query: String): Int!
  task: Task
}

type MutationRoot {
  sendSms(number: String!, priority: Priority): Boolean!
}

enum Priority {
  HIGH
  LOW
}

type Sms {
  id: ID!
  body: String
  tags: [Tag!]!
}

type Tag {
  id: String!
  name: String!
}

union TaskData = A | B

type A {
  x: Int!
}

type B {
  y: Int!
}

type Task {
  data: TaskData
}
`

const EMPTY_EXT = 'schema { query: E }\ntype E { noop: Boolean }'

function schema() {
  return mergeSchemas(parseSchema(SDL), parseSchema(EMPTY_EXT)).schema
}

describe('generateOperationsSource', () => {
  it('types results with nullability, enums, aliases, lists and shared fragments', () => {
    const doc = parseDocument(`
      query list($offset: Int!, $limit: Int!, $query: String!) {
        sms(offset: $offset, limit: $limit, query: $query) {
          ...SmsFragment
        }
        total: smsCount(query: $query)
      }
      fragment SmsFragment on Sms {
        id
        body
        tags { name }
      }
    `)
    const out = generateOperationsSource([{ key: 'listGQL', ir: doc }], schema())
    expect(out).toContain('export interface Fragment_SmsFragment')
    expect(out).toContain('sms: Array<Fragment_SmsFragment>')
    expect(out).toContain('total: number')
    expect(out).toContain('body?: string')
    expect(out).toContain('variables: {\n      offset: number\n      limit: number\n      query: string\n    }')
    expect(out).toContain('export type GqlOperationName = keyof GqlOperations')
  })

  it('types union selections as a union of the inline fragments', () => {
    const doc = parseDocument(`
      query {
        task {
          data {
            ... on A { x }
            ... on B { y }
          }
        }
      }
    `)
    const out = generateOperationsSource([{ key: 'taskGQL', ir: doc }], schema())
    expect(out).toContain("data?: {\n          x: number\n        } | {\n          y: number\n        }")
  })

  it('types nullable enum variables as unions', () => {
    const doc = parseDocument(`
      mutation send($number: String!, $priority: Priority) {
        sendSms(number: $number, priority: $priority)
      }
    `)
    const out = generateOperationsSource([{ key: 'sendGQL', ir: doc }], schema())
    expect(out).toContain("priority: 'HIGH' | 'LOW'")
    expect(out).toContain('sendSms: boolean')
  })

  it('is deterministic', () => {
    const doc = parseDocument('query { smsCount(query: "") }')
    const a = generateOperationsSource([{ key: 'c', ir: doc }], schema())
    const b = generateOperationsSource([{ key: 'c', ir: doc }], schema())
    expect(a).toBe(b)
  })
})
