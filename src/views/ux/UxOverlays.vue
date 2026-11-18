<template>
  <section class="ux-section">
    <h2>Overlays</h2>

    <ux-code-block :code="modalCode">
      <v-filled-button @click="showModal = true">Open Modal</v-filled-button>
      <v-modal v-if="showModal" @close="showModal = false">
        <template #headline>Modal Title</template>
        <template #content>
          <p>This is the modal content area. You can put any content here.</p>
        </template>
        <template #actions>
          <v-outlined-button @click="showModal = false">Cancel</v-outlined-button>
          <v-filled-button @click="showModal = false">Confirm</v-filled-button>
        </template>
      </v-modal>
    </ux-code-block>

    <h3>&lt;v-dropdown&gt; + .dropdown-item</h3>
    <ux-code-block :code="dropdownCode">
      <v-dropdown v-model="dropdownOpen">
        <template #trigger>
          <v-outlined-button @click="dropdownOpen = !dropdownOpen">Open Dropdown</v-outlined-button>
        </template>
        <div class="dropdown-item" @click="dropdownOpen = false">Action One</div>
        <div class="dropdown-item" @click="dropdownOpen = false">Action Two</div>
        <div class="dropdown-item selected" @click="dropdownOpen = false">Selected Item</div>
        <div class="dropdown-item danger" @click="dropdownOpen = false">Danger Item</div>
      </v-dropdown>
    </ux-code-block>

    <ux-code-block code="<!-- Dropdown with icon button trigger -->&#10;<v-dropdown v-model=&quot;open&quot;>&#10;  <template #trigger>&#10;    <v-icon-button>&#10;      <i-material-symbols:more-horiz />&#10;    </v-icon-button>&#10;  </template>&#10;  <div class=&quot;dropdown-item&quot;>Edit</div>&#10;  <div class=&quot;dropdown-item danger&quot;>Delete</div>&#10;</v-dropdown>">
      <v-dropdown v-model="dropdown2Open">
        <template #trigger>
          <v-icon-button><i-material-symbols:more-horiz /></v-icon-button>
        </template>
        <div class="dropdown-item" @click="dropdown2Open = false">Edit</div>
        <div class="dropdown-item danger" @click="dropdown2Open = false">Delete</div>
      </v-dropdown>
    </ux-code-block>
  </section>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import UxCodeBlock from './UxCodeBlock.vue'

const showModal = ref(false)
const dropdownOpen = ref(false)
const dropdown2Open = ref(false)

const modalCode = `<v-modal v-if="show" @close="show = false">
  <template #headline>Title</template>
  <template #content>Content here</template>
  <template #actions>
    <v-outlined-button @click="show = false">Cancel</v-outlined-button>
    <v-filled-button @click="show = false">OK</v-filled-button>
  </template>
</v-modal>`

const dropdownCode = `<v-dropdown v-model="open">
  <template #trigger>
    <v-outlined-button>Menu</v-outlined-button>
  </template>
  <div class="dropdown-item">Action One</div>
  <div class="dropdown-item selected">Selected</div>
  <div class="dropdown-item danger">Danger</div>
</v-dropdown>`
</script>
