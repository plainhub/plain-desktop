import { openSubscriptionConnection, type SubscriptionConnection } from './subscription-connection'
import { fileSearchResultsGQL } from './query'
import type { GqlResultType, GqlVariablesType } from './graphql/operations'

type Page = GqlResultType<'fileSearchResultsGQL'>['fileSearchResults']

export function searchFiles(
  variables: GqlVariablesType<'fileSearchResultsGQL'>,
  next: (page: Page) => void,
  failed: (error: Error) => void,
): () => void {
  let connection: SubscriptionConnection | undefined
  let active = true
  let items: Page['items'] = []
  function stop() {
    active = false
    connection?.close()
  }
  function fail(error: unknown) {
    if (!active) return
    stop()
    failed(error instanceof Error ? error : new Error(String(error)))
  }
  try {
    connection = openSubscriptionConnection({
      opened() {
        if (active) connection?.send({ type: 'subscribe', id: 'search', payload: { query: fileSearchResultsGQL, variables } })
      },
      message(value) {
        if (!active) return
        const message = value as {
          type: string; id?: string
          payload?: { data?: { fileSearchResults?: Page }; errors?: { message: string }[] } | { message: string }[]
        }
        if (message.id !== 'search') return
        if (message.type === 'error') {
          fail(new Error(Array.isArray(message.payload) ? message.payload[0]?.message : 'File search failed'))
        } else if (message.type === 'next' && message.payload && !Array.isArray(message.payload)) {
          if (message.payload.errors?.length) { fail(new Error(message.payload.errors[0].message)); return }
          const page = message.payload.data?.fileSearchResults
          if (page) {
            const offset = Number(page.offset)
            if (offset === 0) items = []
            if (offset !== items.length) { fail(new Error('File search batch out of order')); return }
            items.push(...page.items)
            const expected = Math.min(Math.max(0, Number(page.count) - variables.offset), variables.limit)
            if (items.length === expected) {
              next({ ...page, items })
              items = []
              if (page.complete) stop()
            } else if (page.complete || items.length > expected) fail(new Error('File search batch is incomplete'))
          }
        } else if (message.type === 'complete') {
          if (active) fail(new Error('File search ended before completion'))
        }
      },
      dropped: fail,
    })
  } catch (error) { fail(error) }
  return stop
}
