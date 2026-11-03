import { initMutation, recordMediaPlayGQL, saveMediaPlayProgressGQL } from '@/lib/api/mutation'
import { mediaPlayHistoryGQL } from '@/lib/api/query'
import { getGraphqlContextKey, gqlFetch } from '@/lib/api/gql-client'
import emitter from '@/plugins/eventbus'

type PlaybackMedia = { mediaId: string | null; type: 'AUDIO' | 'VIDEO' }

export function useMediaPlayHistory() {
  const { mutate: recordPlay } = initMutation({ document: recordMediaPlayGQL })
  const { mutate: saveProgress } = initMutation({ document: saveMediaPlayProgressGQL })
  let pending = Promise.resolve()

  function bind(element: HTMLMediaElement, { mediaId, type }: PlaybackMedia, current: () => boolean, onEnded?: () => unknown) {
    const context = getGraphqlContextKey()
    let timer: ReturnType<typeof setInterval> | undefined
    let started = false
    let released = false
    const active = () => !released && current() && context === getGraphqlContextKey() && (!element.currentSrc || element.currentSrc === element.src)
    const save = (record = false) => {
      if (!active() || !mediaId) return
      const variables = { mediaId, type, positionMs: Number.isFinite(element.currentTime) ? Math.max(0, Math.trunc(element.currentTime * 1000)) : 0 }
      pending = pending.then(async () => {
        if (context === getGraphqlContextKey()) await (record ? recordPlay : saveProgress)(variables)
      }).catch(error => emitter.emit('toast', error instanceof Error ? error.message : 'network_error'))
    }
    const stopTimer = () => { if (timer !== undefined) clearInterval(timer); timer = undefined }
    const playing = () => {
      if (!active()) return
      save(!started)
      started = true
      stopTimer()
      timer = setInterval(() => { if (active()) save(); else stopTimer() }, 1000)
    }
    const pause = () => { stopTimer(); if (started) save() }
    const ended = () => {
      stopTimer()
      if (active() && element.ended) {
        if (started) save()
        return onEnded?.()
      }
    }
    element.addEventListener('playing', playing)
    element.addEventListener('pause', pause)
    element.addEventListener('ended', ended, { once: true })
    return () => {
      released = true
      stopTimer()
      element.removeEventListener('playing', playing)
      element.removeEventListener('pause', pause)
      element.removeEventListener('ended', ended)
    }
  }

  async function restorePosition(element: HTMLMediaElement, media: PlaybackMedia | undefined, current: () => boolean) {
    const context = getGraphqlContextKey()
    const active = () => current() && context === getGraphqlContextKey()
    if (media?.mediaId) {
      await pending
      if (!active()) return
      const result = await gqlFetch<{ mediaPlayHistory: Array<{ positionMs: number }> }>(mediaPlayHistoryGQL,
        { query: `type:${media.type} ids:${JSON.stringify(media.mediaId)}`, offset: 0, limit: 1 }, { fresh: true })
      if (!active()) return
      if (result.errors?.length) throw new Error(result.errors[0]!.message)
      element.currentTime = (result.data.mediaPlayHistory[0]?.positionMs ?? 0) / 1000
    }
  }

  async function restoreAndPlay(element: HTMLMediaElement, media: PlaybackMedia | undefined, current: () => boolean) {
    const context = getGraphqlContextKey()
    await restorePosition(element, media, current)
    if (!current() || context !== getGraphqlContextKey()) return
    const release = media ? bind(element, media, current) : undefined
    try { await element.play() }
    catch (error) { release?.(); throw error }
    return release
  }

  return { bind, restorePosition, restoreAndPlay }
}
