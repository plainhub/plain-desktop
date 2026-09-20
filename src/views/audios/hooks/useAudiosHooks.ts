import { initMutation, addAudiosToQueueGQL, removeAudioFromQueueGQL } from '@/lib/api/mutation'
import emitter from '@/plugins/eventbus'
import { ref, type Ref, computed } from 'vue'
import toast from '@/components/toaster'
import { useI18n } from 'vue-i18n'
import type { IAudio } from '@/lib/interfaces'
import { transferEffect } from '@/lib/effect'
import { useAudioPlaylistStore } from '@/hooks/audio-playlist-store'
import { usePlayAudio } from '@/hooks/audio-player'

export const useAddToPlaylist = (items: Ref<IAudio[]>, clearSelection: () => void) => {
  const { mutate, loading, onDone } = initMutation({
    document: addAudiosToQueueGQL,
  })

  const {
    mutate: removeFromPlaylist,
    loading: removeLoading,
    onDone: removeDone,
  } = initMutation({
    document: removeAudioFromQueueGQL,
  })

  const { t } = useI18n()
  const playlistStore = useAudioPlaylistStore()

  onDone(() => {
    playlistStore.refetch()
    clearSelection()
  })

  removeDone(() => {
    playlistStore.refetch()
  })

  const playlistAudios = computed(() => {
    return playlistStore.items.value
  })

  const isInPlaylist = (item: IAudio) => {
    return playlistAudios.value.some((audio) => audio.path === item.path)
  }

  return {
    loading,
    removeLoading,
    isInPlaylist,
    addItemsToPlaylist: (e: MouseEvent, selectedIds: string[], realAllChecked: boolean, query: string) => {
      let q = query
      if (!realAllChecked) {
        if (selectedIds.length === 0) {
          toast(t('select_first'), 'error')
          return
        }
        q = `ids:${selectedIds.join(',')}`
      }
      mutate({ query: q })
    },
    removeItemsFromPlaylist: (e: MouseEvent, selectedIds: string[], realAllChecked: boolean, query: string) => {
      if (!realAllChecked) {
        if (selectedIds.length === 0) {
          toast(t('select_first'), 'error')
          return
        }
        // Remove only selected items that are in playlist
        const selectedItems = items.value.filter((item) => selectedIds.includes(item.id) && isInPlaylist(item))
        selectedItems.forEach((item) => {
          removeFromPlaylist({ path: item.path })
        })
      } else {
        // Remove all items from current query that are in playlist
        items.value
          .filter((item) => isInPlaylist(item))
          .forEach((item) => {
            removeFromPlaylist({ path: item.path })
          })
      }
    },
    addToPlaylist: (e: MouseEvent, item: IAudio) => {
      mutate({ query: `ids:${item.id}` })
    },
    removeFromPlaylist: (e: MouseEvent, item: IAudio) => {
      removeFromPlaylist({ path: item.path })
    },
  }
}

export const useAudioPlayer = () => {
  const playPath = ref('')

  const { play: mutate, loading } = usePlayAudio(() => {
    emitter.emit('do_play_audio')
  })

  return {
    loading,
    playPath,
    play: (item: IAudio) => {
      playPath.value = item.path
      mutate({
        path: item.path,
      })
    },
    pause: () => {
      emitter.emit('pause_audio')
    },
  }
}
