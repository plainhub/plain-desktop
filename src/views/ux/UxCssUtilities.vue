<template>
  <section class="ux-section">
    <h2>CSS Utilities</h2>

    <h3>.skeleton-text / .skeleton-image</h3>
    <ux-code-block code="<div class=&quot;skeleton-text&quot; style=&quot;width: 60%&quot; />&#10;<div class=&quot;skeleton-text lg&quot; style=&quot;width: 40%&quot; />&#10;<div class=&quot;skeleton-image lg&quot; />&#10;<div class=&quot;skeleton-checkbox&quot; />">
      <div style="display: flex; gap: 12px; align-items: center; width: 100%">
        <div class="skeleton-image lg"></div>
        <div style="flex: 1; display: flex; flex-direction: column; gap: 8px;">
          <div class="skeleton-text" style="width: 70%"></div>
          <div class="skeleton-text lg" style="width: 45%"></div>
        </div>
        <div class="skeleton-checkbox"></div>
      </div>
    </ux-code-block>

    <h3>.nowrap</h3>
    <ux-code-block code="<span class=&quot;nowrap&quot;>Text that won't wrap</span>">
      <span class="nowrap">This text will not wrap to a new line, no matter what.</span>
    </ux-code-block>

    <h3>Vue Transitions: fade / width / height</h3>
    <ux-code-block code="<transition name=&quot;fade&quot;>&#10;  <div v-if=&quot;show&quot;>Fades in/out</div>&#10;</transition>&#10;&#10;<transition name=&quot;width&quot;>&#10;  <div v-if=&quot;show&quot;>Width transition</div>&#10;</transition>&#10;&#10;<transition name=&quot;height&quot;>&#10;  <div v-if=&quot;show&quot;>Height transition</div>&#10;</transition>">
      <div style="display: flex; flex-direction: column; gap: 8px; width: 100%">
        <v-outlined-button @click="showTransition = !showTransition">
          Toggle ({{ showTransition ? 'visible' : 'hidden' }})
        </v-outlined-button>
        <div style="display: flex; gap: 16px; align-items: flex-start;">
          <div>
            <div style="font-size: 0.75rem; margin-bottom: 4px; color: var(--md-sys-color-on-surface-variant)">fade:</div>
            <transition name="fade">
              <div v-if="showTransition" class="card" style="padding: 8px">Fade</div>
            </transition>
          </div>
          <div>
            <div style="font-size: 0.75rem; margin-bottom: 4px; color: var(--md-sys-color-on-surface-variant)">width:</div>
            <transition name="width">
              <div v-if="showTransition" class="card" style="padding: 8px; overflow: hidden;">Width</div>
            </transition>
          </div>
          <div>
            <div style="font-size: 0.75rem; margin-bottom: 4px; color: var(--md-sys-color-on-surface-variant)">height:</div>
            <transition name="height">
              <div v-if="showTransition" class="card" style="padding: 8px; overflow: hidden;">Height</div>
            </transition>
          </div>
        </div>
      </div>
    </ux-code-block>

    <h3>.surface-card</h3>
    <ux-code-block code="<div class=&quot;surface-card&quot;>Surface card with background</div>">
      <div class="surface-card" style="flex: 1">Surface card content with default padding</div>
    </ux-code-block>

    <h3>.nav (sidebar navigation)</h3>
    <ux-code-block code="<ul class=&quot;nav&quot;>&#10;  <li class=&quot;active&quot;>&#10;    <div class=&quot;icon&quot;>📁</div>&#10;    <div class=&quot;title&quot;>Files</div>&#10;    <div class=&quot;count&quot;>12</div>&#10;  </li>&#10;</ul>">
      <ul class="nav" style="width: 200px">
        <li class="active">
          <div class="icon"><i-lucide:folder /></div>
          <div class="title">Files</div>
          <div class="count">12</div>
        </li>
        <li>
          <div class="icon"><i-lucide:image /></div>
          <div class="title">Images</div>
          <div class="count">48</div>
        </li>
        <li>
          <div class="icon"><i-lucide:music /></div>
          <div class="title">Audio</div>
        </li>
      </ul>
    </ux-code-block>

    <h3>.pagination</h3>
    <ux-code-block code="<v-pagination :total=&quot;100&quot; :limit=&quot;10&quot; :page=&quot;3&quot; :go=&quot;goToPage&quot; />">
      <v-pagination :total="100" :limit="10" :page="currentPage" :go="(p: number) => currentPage = p" />
    </ux-code-block>
  </section>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import UxCodeBlock from './UxCodeBlock.vue'

const showTransition = ref(true)
const currentPage = ref(3)
</script>
