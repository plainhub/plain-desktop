import { computed, ref } from 'vue'

const windowHeight = ref(window.innerHeight)
const isPhone = ref(window.innerWidth <= 480)
const isTablet = ref(window.innerWidth <= 768)

function update() {
  windowHeight.value = window.innerHeight
  isPhone.value = window.innerWidth <= 480
  isTablet.value = window.innerWidth <= 768
}

window.addEventListener('resize', update)

export function getIsTablet() {
  return isTablet
}

export function getIsPhone() {
  return isPhone
}

export function useVirtualRowCount(estimatedRowHeight: number) {
  return computed(() => Math.max(1, Math.ceil(windowHeight.value / estimatedRowHeight)) * 3)
}
