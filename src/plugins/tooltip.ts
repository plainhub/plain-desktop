import type { Directive, Plugin } from 'vue'

declare global {
  var showTooltipTimeout: ReturnType<typeof setTimeout>
}

const plugin: Plugin = {
  install: (app) => {
    app.directive('tooltip', directive)
  },
}

const directive: Directive = {
  updated: (el, binding) => {
    // save the new value so we can use later
    el.tooltipText = binding.value
  },
  mounted: (el, binding) => {
    let tooltipElement: HTMLDivElement | null = null

    function createTooltip() {
      tooltipElement = document.createElement('div')
      tooltipElement.className = 'tooltip'
      tooltipElement.textContent = el.tooltipText || binding.value
      const fullscreenElement = document.fullscreenElement
      if (fullscreenElement) {
        fullscreenElement.appendChild(tooltipElement)
      } else {
        document.body.appendChild(tooltipElement)
      }

      // Set initial styles for the tooltip
      if (tooltipElement) {
        tooltipElement.style.position = 'absolute'
        tooltipElement.style.visibility = 'hidden'
      }
    }

    function showTooltip() {
      if (!tooltipElement) {
        createTooltip()
      }

      const rect = el.getBoundingClientRect()
      if (rect.x === 0 && rect.y === 0) {
        // invalid
        return
      }
      if (tooltipElement) {
        const tooltipRect = tooltipElement.getBoundingClientRect()
        tooltipElement.style.top = rect.bottom + 8 + 'px'
        tooltipElement.style.left = rect.left + (rect.width - tooltipRect.width) / 2 + 'px'
        tooltipElement.style.visibility = 'visible'
      }
    }

    el.mouseenterFunc = () => {
      globalThis.showTooltipTimeout = setTimeout(showTooltip, 600)
    }

    el.addEventListener('mouseenter', el.mouseenterFunc)

    el.mouseleaveFunc = () => {
      clearTimeout(globalThis.showTooltipTimeout)
      const tooltips = Array.from(document.getElementsByClassName('tooltip'))
      for (const tooltip of tooltips) {
        if (tooltip.parentNode) {
          tooltip.parentNode.removeChild(tooltip)
        }
      }
      tooltipElement = null
    }

    if (el.tagName === 'BUTTON' && !el.className.includes('btn-help')) {
      el.addEventListener('click', el.mouseleaveFunc)
    }
    el.addEventListener('mouseleave', el.mouseleaveFunc)
  },
  unmounted: (el) => {
    el.mouseleaveFunc()
    el.removeEventListener('mouseenter', el.mouseenterFunc)
    if (el.tagName === 'BUTTON') {
      el.removeEventListener('click', el.mouseleaveFunc)
    }
    el.removeEventListener('mouseleave', el.mouseleaveFunc)
  },
}

export default plugin
