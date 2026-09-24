export function transferEffect(source: any, target: any, durationMs = 500) {
  const clone = source.cloneNode(true)

  const sourceRect = source.getBoundingClientRect()
  const targetRect = target.getBoundingClientRect()

  clone.style.position = 'absolute'
  clone.style.top = sourceRect.top + 'px'
  clone.style.left = sourceRect.left + 'px'
  clone.style.opacity = 1

  document.body.appendChild(clone)

  let startTime = 0

  function animate(currentTime: number) {
    if (!startTime) {
      startTime = currentTime
    }

    const elapsed = currentTime - startTime
    const progress = Math.min(elapsed / durationMs, 1)

    clone.style.top = sourceRect.top + (targetRect.top - sourceRect.top) * progress + 'px'
    clone.style.left = sourceRect.left + (targetRect.left - sourceRect.left) * progress + 'px'

    if (progress < 1) {
      requestAnimationFrame(animate)
    } else {
      document.body.removeChild(clone)
    }
  }

  requestAnimationFrame(animate)
}
