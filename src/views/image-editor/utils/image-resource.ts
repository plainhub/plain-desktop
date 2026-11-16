export function serializeEditorImage(image: HTMLImageElement): string {
  const canvas = document.createElement('canvas')
  canvas.width = image.naturalWidth
  canvas.height = image.naturalHeight
  canvas.getContext('2d')!.drawImage(image, 0, 0)
  return canvas.toDataURL('image/png')
}
