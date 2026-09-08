<template>
  <div
    v-if="items"
    :class="'context-menu ' + (options.customClass ? options.customClass : '') + (menuReady ? ' ready' : '')"
    :style="{
      maxWidth: parentItem && parentItem.maxWidth ? `${parentItem.maxWidth}px` : `600px`,
      minWidth: parentItem && parentItem.minWidth ? `${parentItem.minWidth}px` : `100px`,
      zIndex: zIndex,
      left: `${position.x}px`,
      top: `${position.y}px`,
    }"
    @mouseenter="onMenuMouseEnter"
    @mouseleave="onMenuMouseLeave($event)"
  >
    <div v-show="menuOverflow" class="context-menu-updown up" @click="onScroll(false)">
      <span class="right-arrow" />
    </div>
    <div v-show="menuOverflow" class="context-menu-updown down" @click="onScroll(true)">
      <span class="right-arrow" />
    </div>
    <div
      ref="menu"
      class="context-menu-items"
      :style="{
        maxHeight: maxHeight > 0 ? `${maxHeight}px` : '',
      }"
    >
      <div
        v-for="(item, i) in items"
        :key="i"
        class="dropdown-item"
        :class="[{ disabled: item.disabled, divided: item.divided }, item.customClass]"
        @mouseenter="showChildItem($event, item)"
        @mouseleave="hideChildItem()"
        @focus="showChildItem($event, item)"
        @blur="hideChildItem()"
        @click="onMouseClick(item)"
      >
        <component :is="item.icon" v-if="item.icon" class="dropdown-item-icon" />
        <span>{{ item.label }}</span>
      </div>
    </div>
    <ContextSubMenu
      v-if="activeItem && activeItem.children"
      ref="childMenu"
      :z-index="zIndex + 1"
      :items="activeItem.children"
      :parent-item="activeItem"
      :options="options"
      :global-data="childGlobalData"
      :position="childPosition"
      @close="onChildrenClose"
      @keep-open="onChildrenKeepOpen"
      @pre-update-pos="onChildrenUpdatePos"
    ></ContextSubMenu>
  </div>
</template>

<script lang="ts" setup>
import { onBeforeUnmount, onMounted, type PropType, ref, toRefs, watch } from 'vue'
import type { MenuOptions, MenuItem, ContextMenuGlobalData, ContextMenuPositionData } from './ContextMenuDefine'

const props = defineProps({
  items: {
    type: Object as PropType<Array<MenuItem>>,
    default: null,
  },
  parentItem: {
    type: Object as PropType<MenuItem>,
    default: null,
  },
  options: {
    type: Object as PropType<MenuOptions>,
    default: null,
  },
  zIndex: {
    type: Number,
    default: 0,
  },
  globalData: {
    type: Object as PropType<ContextMenuGlobalData>,
    default: null,
  },
  position: {
    type: Object as PropType<ContextMenuPositionData>,
    default: null,
  },
})

const emit = defineEmits(['close', 'keepOpen', 'preUpdatePos'])
const { globalData, position, options, parentItem } = toRefs(props)

const menu = ref<HTMLElement>()
const childMenu = ref()
const menuReady = ref(false)
const menuOverflow = ref(false)

let nextShouldHideItem = null as MenuItem | null
const maxHeight = ref(0)
const activeItem = ref<MenuItem | null>(null)
const childGlobalData = ref({
  parentPosition: { x: 0, y: 0 },
  screenSize: globalData.value.screenSize,
} as ContextMenuGlobalData)
const childPosition = ref<ContextMenuPositionData>({
  x: 0,
  y: 0,
})

//显示和隐藏子菜单
function showChildItem(e: Event, item: MenuItem) {
  if (item.disabled || !item.children || item.children.length == 0) return
  if (activeItem.value === item) return

  //同步父菜单的位置
  activeItem.value = item
  childGlobalData.value.parentPosition.x = globalData.value.parentPosition.x + position.value.x
  childGlobalData.value.parentPosition.y = globalData.value.parentPosition.y + position.value.y

  //计算子菜单的位置
  if (menu.value) childPosition.value.x = menu.value.offsetWidth + (options.value.xOffset || 0)
  const currentItemEle = e.target as HTMLElement
  if (currentItemEle) childPosition.value.y = currentItemEle.offsetTop + (options.value.yOffset || 0)
}
function hideChildItem() {
  nextShouldHideItem = activeItem.value
  setTimeout(() => {
    if (nextShouldHideItem === activeItem.value) activeItem.value = null
  })
}

watch(activeItem, (newV: MenuItem | null, oldV: MenuItem | null) => {
  if (newV && oldV) {
    setTimeout(() => {
      if (childMenu.value) childMenu.value.doCheckPos()
    }, 50)
  }
})

//子菜单事件
function onChildrenClose(byUserClick: boolean) {
  hideChildItem()
  if (byUserClick) emit('close', true)
}
function onChildrenKeepOpen(item: MenuItem) {
  if (nextShouldHideItem === item) nextShouldHideItem = null
  emit('keepOpen', parentItem.value)
}
function onChildrenUpdatePos(newPos: ContextMenuPositionData) {
  childPosition.value.x = newPos.x
  childPosition.value.y = newPos.y
}
//鼠标事件
function onMouseClick(item: MenuItem) {
  if (!item.disabled) {
    if (typeof item.onClick === 'function') {
      item.onClick()
      emit('close', true)
    } else if (!item.children || item.children.length === 0) {
      emit('close', true)
    }
  }
}
function onMenuMouseEnter() {
  emit('keepOpen', parentItem.value)
}
function onMenuMouseLeave(e: MouseEvent) {
  if (e.relatedTarget != null) emit('close', false)
}

//滚动
function onScroll(down: boolean) {
  if (menu.value) {
    menu.value.scrollTop += down ? 30 : -30
  }
}

let solveOverflowTimeOut = 0

//检查菜单是否超出屏幕
function doCheckPos() {
  const _menu = menu.value
  const _globalData = globalData.value
  if (_menu) {
    const newPos = {
      x: position.value.x,
      y: position.value.y,
    } as ContextMenuPositionData

    //如果X绝对位置超出屏幕，那么减去超出的宽度
    const absRight = _globalData.parentPosition.x + position.value.x + _menu.offsetWidth
    if (absRight > _globalData.screenSize.w) {
      newPos.x -= absRight - _globalData.screenSize.w
    }

    //如果高度超出屏幕，那么限制最高高度
    if (_menu.offsetHeight > _globalData.screenSize.h - 30) {
      maxHeight.value = _globalData.screenSize.h - 30
      //  强制限制Y坐标为0
      newPos.y = -_globalData.parentPosition.y
      menuOverflow.value = true
    } else {
      menuOverflow.value = false
      maxHeight.value = 0
      //如果Y绝对位置超出屏幕，那么减去超出的高度
      const absTop = _globalData.parentPosition.y + position.value.y + _menu.offsetHeight
      if (absTop > _globalData.screenSize.h) {
        newPos.y -= absTop - _globalData.screenSize.h + 30
      }
    }

    emit('preUpdatePos', newPos)
    menuReady.value = true
  }
}

onMounted(() => {
  solveOverflowTimeOut = window.setTimeout(() => doCheckPos(), 100)
})

onBeforeUnmount(() => {
  if (solveOverflowTimeOut > 0) {
    clearTimeout(solveOverflowTimeOut)
    solveOverflowTimeOut = 0
  }
})
</script>

<style lang="scss" scoped>
.context-menu {
  display: inline-block;
  overflow: visible;
  position: fixed;
  background-color: var(--md-sys-color-surface-container);
  border-radius: 8px;
  box-shadow: rgba(0, 0, 0, 0.2) 0px 2px 6px;
  border: 1px solid rgba(0, 0, 0, 0.08);
  z-index: 1000;
  min-width: 136px;
  opacity: 0;
  white-space: nowrap;
}

.context-menu.ready {
  opacity: 1;
}

.context-menu-items {
  position: relative;
  overflow-y: auto;
  border-radius: 8px;
  max-height: 60vh;
  max-width: 280px;
  padding: 4px;

  &::-webkit-scrollbar {
    width: 6px;
  }

  &::-webkit-scrollbar-track {
    background: transparent;
  }

  &::-webkit-scrollbar-thumb {
    background: var(--md-sys-color-outline);
    border-radius: 3px;
  }
}

.context-menu .dropdown-item {
  border-radius: 6px;
}

.context-menu-updown {
  position: absolute;
  left: 0;
  right: 0;
  height: 10px;
  box-shadow: var(--md-sys-color-shadow) 0px 1px 2px 0px;
  background-color: var(--md-sys-color-surface-container);
  border-radius: 4px;
  user-select: none;
  cursor: pointer;

  & .right-arrow {
    display: inline-block;
    position: absolute;
    height: 12px;
    left: 50%;
    top: 0;
  }
}

.context-menu-updown {
  &.up {
    top: 0;
  }

  &.down {
    bottom: 0;
  }
}

.context-menu-updown.up .right-arrow {
  transform: translateX(-50%) rotate(270deg);
}

.context-menu-updown.down .right-arrow {
  transform: translateX(-50%) rotate(90deg);
}

.right-arrow {
  width: 14px;
  height: 14px;
  background-image: url('data:image/svg+xml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBzdGFuZGFsb25lPSJubyI/PjwhRE9DVFlQRSBzdmcgUFVCTElDICItLy9XM0MvL0RURCBTVkcgMS4xLy9FTiIgImh0dHA6Ly93d3cudzMub3JnL0dyYXBoaWNzL1NWRy8xLjEvRFREL3N2ZzExLmR0ZCI+PHN2ZyB0PSIxNjI1MjA3MjM5MzE1IiBjbGFzcz0iaWNvbiIgdmlld0JveD0iMCAwIDEwMjQgMTAyNCIgdmVyc2lvbj0iMS4xIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHAtaWQ9IjIxMjYzIiB4bWxuczp4bGluaz0iaHR0cDovL3d3dy53My5vcmcvMTk5OS94bGluayIgd2lkdGg9IjUwIiBoZWlnaHQ9IjUwIj48ZGVmcz48c3R5bGUgdHlwZT0idGV4dC9jc3MiPjwvc3R5bGU+PC9kZWZzPjxwYXRoIGQ9Ik0zMDcuMDE4IDQ5LjQ0NWMxMS41MTcgMCAyMy4wMzIgNC4zOTQgMzEuODE5IDEzLjE4TDc1Ni40MDQgNDgwLjE4YzguNDM5IDguNDM4IDEzLjE4MSAxOS44ODUgMTMuMTgxIDMxLjgycy00Ljc0MSAyMy4zOC0xMy4xODEgMzEuODJMMzM4LjgzOCA5NjEuMzc2Yy0xNy41NzQgMTcuNTczLTQ2LjA2NSAxNy41NzMtNjMuNjQtMC4wMDEtMTcuNTczLTE3LjU3My0xNy41NzMtNDYuMDY1IDAuMDAxLTYzLjY0TDY2MC45NDQgNTEyIDI3NS4xOTggMTI2LjI2NWMtMTcuNTc0LTE3LjU3My0xNy41NzQtNDYuMDY2LTAuMDAxLTYzLjY0QzI4My45ODUgNTMuODM5IDI5NS41MDEgNDkuNDQ1IDMwNy4wMTggNDkuNDQ1eiIgcC1pZD0iMjEyNjQiPjwvcGF0aD48L3N2Zz4=');
  background-size: 12px;
  background-repeat: no-repeat;
}
</style>
