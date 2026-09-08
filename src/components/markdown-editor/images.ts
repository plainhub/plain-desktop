import { useTempStore } from '@/stores/temp'
import { useOpenMedia } from '@/hooks/open-media'
import { getFileUrlByPath } from '@/lib/api/file'
import { DataType } from '@/lib/data'

export function resolveImageUrl(link: string): string {
  const { app, urlTokenKey } = useTempStore()
  if (link.startsWith('app://')) {
    return getFileUrlByPath(urlTokenKey, app.appDir + '/' + link.replace('app://', ''))
  }
  if (link.startsWith('fid:')) {
    return getFileUrlByPath(urlTokenKey, link)
  }
  return link
}

export function zoomImage(link: string) {
  const { open } = useOpenMedia()
  open(0, [{
    src: resolveImageUrl(link),
    path: link.replace(/^app:\/\//, ''),
    name: decodeURIComponent(link.split('/').pop() ?? 'image'),
    size: 0,
    duration: 0,
    type: DataType.IMAGE,
  }], true)
}
