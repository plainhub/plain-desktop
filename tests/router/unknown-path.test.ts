import { describe, expect, it } from 'vitest'
import router from '@/plugins/router'

/**
 * An unknown path used to match no route at all, so the router rendered an
 * empty shell: a white page with no explanation and no way back, which is how
 * a typo'd or stale link (/audio when the real route is /audios) looks like a
 * broken app. The catch-all added at the end of the route table bounces those
 * home.
 *
 * Asserted against the real router, not a copy of the table -- a guard built
 * from a hand-written list would pass even after the table itself regressed.
 */
const CATCH_ALL = '/:pathMatch(.*)*'

function matchedPaths(path: string): string[] {
  return router.resolve(path).matched.map((record) => record.path)
}

describe('unknown paths', () => {
  it('falls back to the catch-all instead of matching nothing', () => {
    for (const path of ['/audio', '/nope', '/definitely-not-a-route', '/audios/typo/deep']) {
      expect(matchedPaths(path), path).toContain(CATCH_ALL)
    }
  })

  it('leaves the real routes alone', () => {
    // /audios is the actual name; if this ever resolves to the catch-all the
    // redirect has swallowed a working page instead of just the typo'd one.
    const matched = matchedPaths('/audios')
    expect(matched).toContain('/audios')
    expect(matched).not.toContain(CATCH_ALL)
  })

  it('redirects rather than rendering a component of its own', () => {
    const record = router.resolve('/audio').matched.at(-1)!
    expect(record.redirect).toBeTruthy()
  })
})
