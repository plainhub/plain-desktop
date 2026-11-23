const previews = new Map<string, ImageData>()

export function thumbHashPreview(encoded: string): ImageData | undefined {
  const cached = previews.get(encoded)
  if (cached) {
    previews.delete(encoded)
    previews.set(encoded, cached)
    return cached
  }
  if (encoded.length < 8 || encoded.length > 36) return
  let hash: Uint8Array
  try {
    hash = Uint8Array.from(atob(encoded), char => char.charCodeAt(0))
  } catch {
    return
  }
  if (hash.length < 5) return

  const luminance = hash[0] | (hash[1] << 8) | (hash[2] << 16)
  const chroma = hash[3] | (hash[4] << 8)
  const hasAlpha = (luminance & 0x800000) !== 0
  const landscape = (chroma & 0x8000) !== 0
  const major = hasAlpha ? 5 : 7
  const minor = chroma & 7
  if (minor < 1 || minor > major) return

  const lx = landscape ? major : minor
  const ly = landscape ? minor : major
  const width = Math.round(32 * lx / major)
  const height = Math.round(32 * ly / major)
  const basis = (length: number) => Array.from({ length }, (_, pixel) =>
    Array.from({ length: 7 }, (_, term) => Math.cos(Math.PI / length * (pixel + 0.5) * term)),
  )
  const bx = basis(width)
  const by = basis(height)
  const start = hasAlpha ? 6 : 5
  let term = 0
  function channel(nx: number, ny: number, dc: number, scale: number) {
    const values: { cx: number; cy: number; weight: number }[] = []
    for (let cy = 0; cy < ny; cy++) {
      for (let cx = cy === 0 ? 1 : 0; cx * ny + cy * nx < nx * ny; cx++) {
        const packed = hash[start + (term >> 1)]
        const nibble = (packed >> ((term & 1) * 4)) & 15
        term++
        values.push({ cx, cy, weight: (nibble / 7.5 - 1) * scale * 2 })
      }
    }
    return (x: number, y: number) => {
      let value = dc
      for (const { cx, cy, weight } of values) value += weight * bx[x][cx] * by[y][cy]
      return value
    }
  }
  const l = channel(Math.max(3, lx), Math.max(3, ly), (luminance & 63) / 63, ((luminance >> 18) & 31) / 31)
  const p = channel(3, 3, ((luminance >> 6) & 63) / 31.5 - 1, ((chroma >> 3) & 63) / 63 * 1.25)
  const q = channel(3, 3, ((luminance >> 12) & 63) / 31.5 - 1, ((chroma >> 9) & 63) / 63 * 1.25)
  const alpha = hasAlpha ? channel(5, 5, (hash[5] & 15) / 15, (hash[5] >> 4) / 15) : () => 1
  if (hash.length !== start + Math.ceil(term / 2)) return

  const image = new ImageData(width, height)
  let index = 0
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const light = l(x, y)
      const color = q(x, y)
      const blue = light - (2 / 3) * p(x, y)
      const red = (3 * light - blue + color) / 2
      image.data[index++] = Math.trunc(red * 255)
      image.data[index++] = Math.trunc((red - color) * 255)
      image.data[index++] = Math.trunc(blue * 255)
      image.data[index++] = Math.trunc(alpha(x, y) * 255)
    }
  }
  previews.set(encoded, image)
  if (previews.size > 128) previews.delete(previews.keys().next().value!)
  return image
}
