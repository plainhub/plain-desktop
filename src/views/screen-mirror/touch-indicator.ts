/** Touch feedback dot rendered on the overlay while a pointer is down. */

// With reduced motion the global transition disable means transitionend never
// fires — clean up synchronously there instead of arming a 300ms fallback
// timer per tap.
const REDUCED_MOTION =
  typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches

export function createTouchIndicator(container: HTMLElement): HTMLDivElement {
  const dot = document.createElement('div')
  dot.className = 'touch-indicator'
  container.appendChild(dot)
  return dot
}

export function positionIndicator(dot: HTMLDivElement, x: number, y: number) {
  dot.style.left = x + 'px'
  dot.style.top = y + 'px'
}

export function showIndicator(dot: HTMLDivElement, x: number, y: number) {
  positionIndicator(dot, x, y)
  dot.classList.remove('touch-indicator--fade-out')
  dot.classList.add('touch-indicator--active')
}

export function hideIndicator(dot: HTMLDivElement, isTap: boolean, remove: boolean) {
  if (isTap) dot.classList.add('touch-indicator--ripple')
  dot.classList.remove('touch-indicator--active')
  dot.classList.add('touch-indicator--fade-out')
  // transitionend + timer belt-and-braces: reduced motion disables the
  // transition globally, so transitionend alone would leak one node per tap
  let done = false
  const cleanup = () => {
    if (done) return
    done = true
    dot.classList.remove(
      'touch-indicator--fade-out',
      'touch-indicator--ripple',
      'touch-indicator--dragging',
      'touch-indicator--long-press',
    )
    if (remove) dot.remove()
  }
  if (REDUCED_MOTION) {
    dot.classList.remove('touch-indicator--ripple', 'touch-indicator--dragging', 'touch-indicator--long-press')
    if (remove) dot.remove()
    return
  }
  dot.addEventListener('transitionend', cleanup, { once: true })
  setTimeout(cleanup, 300)
}
