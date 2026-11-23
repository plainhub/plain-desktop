import { expect, it } from 'vitest'
import { imagesGQL, mediaBucketsGQL, videosGQL } from '@/lib/api/query'

it('requests the placeholder hash for image, video, and folder thumbnails', () => {
  expect(imagesGQL).toMatch(/fragment ImageFragment on Image\s*\{[^{}]*\bthumbHash\b/)
  expect(videosGQL).toMatch(/fragment VideoFragment on Video\s*\{[^{}]*\bthumbHash\b/)
  expect(mediaBucketsGQL).toMatch(/coverItems\s*\{[^{}]*\bthumbHash\b/)
})
