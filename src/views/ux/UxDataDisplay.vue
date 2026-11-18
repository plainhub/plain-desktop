<template>
  <section class="ux-section">
    <h2>Chips &amp; Data Display</h2>

    <h3>&lt;v-filter-chip&gt; + &lt;v-chip-set&gt;</h3>
    <ux-code-block code="<v-chip-set>&#10;  <v-filter-chip label=&quot;All&quot; :selected=&quot;filter === 'all'&quot; @click=&quot;filter = 'all'&quot; />&#10;  <v-filter-chip label=&quot;Active&quot; :selected=&quot;filter === 'active'&quot; @click=&quot;filter = 'active'&quot; />&#10;</v-chip-set>">
      <v-chip-set>
        <v-filter-chip label="All" :selected="filter === 'all'" @click="filter = 'all'" />
        <v-filter-chip label="Active" :selected="filter === 'active'" @click="filter = 'active'" />
        <v-filter-chip label="Archived" :selected="filter === 'archived'" @click="filter = 'archived'" />
      </v-chip-set>
    </ux-code-block>

    <h3>&lt;v-input-chip&gt;</h3>
    <ux-code-block code="<v-input-chip label=&quot;Tag&quot; @remove=&quot;...&quot; />">
      <v-input-chip label="Photo" @remove="() => {}" />
      <v-input-chip label="Video" selected @remove="() => {}" />
      <v-input-chip label="Disabled" disabled />
    </ux-code-block>

    <h3>CSS: .card / .card.outlined</h3>
    <ux-code-block code="<div class=&quot;card&quot;>Default card</div>&#10;<div class=&quot;card outlined&quot;>Outlined card</div>&#10;<div class=&quot;card selected&quot;>Selected card</div>">
      <div class="card" style="flex: 1">Default card content</div>
      <div class="card outlined" style="flex: 1">Outlined card content</div>
      <div class="card selected" style="flex: 1">Selected card content</div>
    </ux-code-block>

    <h3>CSS: .selectable-card</h3>
    <ux-code-block code="<div class=&quot;selectable-card&quot;>Normal</div>&#10;<div class=&quot;selectable-card selected&quot;>Selected</div>&#10;<div class=&quot;selectable-card selecting&quot;>Selecting</div>">
      <div class="selectable-card" style="padding: 12px; flex: 1">Normal</div>
      <div class="selectable-card selected" style="padding: 12px; flex: 1">Selected</div>
      <div class="selectable-card selecting" style="padding: 12px; flex: 1">Selecting</div>
    </ux-code-block>

    <h3>CSS: .top-app-bar</h3>
    <ux-code-block code="<div class=&quot;top-app-bar&quot;>&#10;  <v-icon-button>...</v-icon-button>&#10;  <div class=&quot;title&quot;>Page Title</div>&#10;  <div class=&quot;actions&quot;>...</div>&#10;</div>">
      <div class="top-app-bar" style="width: 100%">
        <v-icon-button><i-material-symbols:arrow-back-rounded /></v-icon-button>
        <div class="title">Page Title</div>
        <div class="actions">
          <v-icon-button><i-material-symbols:search-rounded /></v-icon-button>
          <v-icon-button><i-material-symbols:more-horiz /></v-icon-button>
        </div>
      </div>
    </ux-code-block>
  </section>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import UxCodeBlock from './UxCodeBlock.vue'

const filter = ref('all')
</script>
