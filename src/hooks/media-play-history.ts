import { HttpErrorCode } from '@/lib/api/http-error-code'
import { initMutation, recordMediaPlayGQL, saveMediaPlayProgressGQL } from '@/lib/api/mutation'
import { mediaPlayHistoryGQL } from '@/lib/api/query'
import { getGraphqlContextKey, gqlFetch } from '@/lib/api/gql-client'
import emitter from '@/plugins/eventbus'
import { isLocalMode } from '@/lib/device/local-mode'

type PlaybackMedia = { mediaId: string | null; type: 'AUDIO' | 'VIDEO' }

export function useMediaPlayHistory() {
  const { mutate: recordPlay } = initMutation({ document: recordMediaPlayGQL })
  const { mutate: saveProgress } = initMutation({ document: saveMediaPlayProgressGQL })
  let lastWrite = Promise.resolve()
  const pendingWrites = new Map<string, Promise<void>>()
  const historyKey = (context: string, media: PlaybackMedia) => JSON.stringify([context, media.type, media.mediaId])

  function bind(element: HTMLMediaElement, { mediaId, type }: PlaybackMedia, current: () => boolean, onEnded?: () => unknown) {
    const context = getGraphqlContextKey()
    const key = historyKey(context, { mediaId, type })
    let timer: ReturnType<typeof setInterval> | undefined
    let started = false
    let released = false
    const active = () => !released && current() && context === getGraphqlContextKey() && (!element.currentSrc || element.currentSrc === element.src)
    const save = (record = false) => {
      if (!active() || !mediaId || (!record && !isLocalMode())) return
      const variables = { mediaId, type, positionMs: Number.isFinite(element.currentTime) ? Math.max(0, Math.trunc(element.currentTime * 1000)) : 0 }
      const write = lastWrite.then(async () => {
        if (context === getGraphqlContextKey()) await (record ? recordPlay : saveProgress)(variables)
      }).catch(error => emitter.emit('toast', error instanceof Error ? error.message : HttpErrorCode.NetworkError))
      lastWrite = write
      pendingWrites.set(key, write)
      void write.then(() => { if (pendingWrites.get(key) === write) pendingWrites.delete(key) })
    }
    const stopTimer = () => { if (timer !== undefined) clearInterval(timer); timer = undefined }
    const playing = () => {
      if (!active()) return
      save(!started)
      started = true
      stopTimer()
      if (isLocalMode()) timer = setInterval(() => { if (active()) save(); else stopTimer() }, 1000)
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

  async function readPosition(media: PlaybackMedia | undefined, current: () => boolean) {
    const context = getGraphqlContextKey()
    const active = () => current() && context === getGraphqlContextKey()
    if (media?.mediaId) {
      await pendingWrites.get(historyKey(context, media))
      if (!active()) return
      const result = await gqlFetch<{ mediaPlayHistory: Array<{ positionMs: number }> }>(mediaPlayHistoryGQL,
        { query: `type:${media.type} ids:${JSON.stringify(media.mediaId)}`, offset: 0, limit: 1 }, { fresh: true })
      if (!active()) return
      if (result.errors?.length) throw new Error(result.errors[0]!.message)
      return (result.data.mediaPlayHistory[0]?.positionMs ?? 0) / 1000
    }
  }

  function applyPosition(element: HTMLMediaElement, position: number | undefined) {
    if (position !== undefined && element.currentTime !== position) element.currentTime = position
  }

  async function restorePosition(element: HTMLMediaElement, media: PlaybackMedia | undefined, current: () => boolean) {
    const context = getGraphqlContextKey()
    const position = await readPosition(media, current)
    if (position !== undefined && current() && context === getGraphqlContextKey()) element.currentTime = position
  }

  async function restoreAndPlay(ready: HTMLMediaElement | Promise<HTMLMediaElement | undefined>, media: PlaybackMedia | undefined, current: () => boolean) {
    const context = getGraphqlContextKey()
    const [element, result] = await Promise.all([
      ready,
      readPosition(media, current).then(position => ({ position }), (error: unknown) => ({ error })),
    ])
    if (!element || !current() || context !== getGraphqlContextKey()) return
    if ('error' in result) throw result.error
    applyPosition(element, result.position)
    const release = media ? bind(element, media, current) : undefined
    try { await element.play() }
    catch (error) { release?.(); throw error }
    return release
  }

  return { bind, restorePosition, restoreAndPlay }
}
