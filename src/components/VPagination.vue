<template>
  <div class="pagination">
    <button class="btn-icon" :disabled="!isPrevActive" @click.prevent="prev">
      <i-material-symbols:skip-previous-outline-rounded />
    </button>
    <template v-for="(index, i) in pagination" :key="i">
      <span v-if="index === null" class="page-link">···</span>
      <button v-else class="btn-icon page-number" :class="{ active: index === props.page }" @click.prevent="go(index)">
        {{ index }}
      </button>
    </template>
    <button class="btn-icon" :disabled="!isNextActive" @click.prevent="next">
      <i-material-symbols:skip-next-outline-rounded />
    </button>
    <template v-if="onChangePageSize != null">
      <span class="page-size-divider"></span>
      <select class="page-size-select" :value="pageSize" @change="handlePageSizeChange">
        <option v-for="size in pageSizeOptions" :key="size" :value="size">{{ size }}</option>
      </select>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, type PropType } from 'vue'

const props = defineProps({
  total: {
    type: Number,
    default: 0,
  },
  limit: {
    type: Number,
    default: 50,
  },
  rangeSize: {
    type: Number,
    default: 2,
  },
  go: {
    type: Function as PropType<(value: number) => void>,
    required: true,
  },
  page: {
    type: Number,
    default: 1,
  },
  pageSize: {
    type: Number,
    default: 50,
  },
  pageSizeOptions: {
    type: Array as PropType<number[]>,
    default: () => [20, 50, 100, 200],
  },
  onChangePageSize: {
    type: Function as PropType<(size: number) => void>,
    default: undefined,
  },
})

const pages = computed(() => {
  return Math.ceil(props.total / props.limit)
})

const pagination = computed((): (number | null)[] => {
  const res = []
  const minPaginationElems = 5 + props.rangeSize * 2

  let rangeStart = pages.value <= minPaginationElems ? 1 : props.page - props.rangeSize
  let rangeEnd = pages.value <= minPaginationElems ? pages.value : props.page + props.rangeSize

  rangeEnd = rangeEnd > pages.value ? pages.value : rangeEnd
  rangeStart = rangeStart < 1 ? 1 : rangeStart

  if (pages.value > minPaginationElems) {
    const isStartBoundaryReached = rangeStart - 1 < 3
    const isEndBoundaryReached = pages.value - rangeEnd < 3

    if (isStartBoundaryReached) {
      rangeEnd = minPaginationElems - 2
      for (let i = 1; i < rangeStart; i++) {
        res.push(i)
      }
    } else {
      res.push(1)
      res.push(null)
    }

    if (isEndBoundaryReached) {
      rangeStart = pages.value - (minPaginationElems - 3)
      for (let i = rangeStart; i <= pages.value; i++) {
        res.push(i)
      }
    } else {
      for (let i = rangeStart; i <= rangeEnd; i++) {
        res.push(i)
      }
      res.push(null)
      res.push(pages.value)
    }
  } else {
    for (let i = rangeStart; i <= rangeEnd; i++) {
      res.push(i)
    }
  }

  return res
})

const isPrevActive = computed((): boolean => {
  return props.page > 1
})

const isNextActive = computed((): boolean => {
  return props.page < pages.value
})

function prev(): void {
  if (isPrevActive.value) {
    props.go(props.page - 1)
  }
}

function next(): void {
  if (isNextActive.value) {
    props.go(props.page + 1)
  }
}

function handlePageSizeChange(event: Event): void {
  const size = Number((event.target as HTMLSelectElement).value)
  props.onChangePageSize?.(size)
}
</script>
