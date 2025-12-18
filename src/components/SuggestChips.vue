<script setup lang="ts">
defineProps<{
  suggestions: string[]
}>()

const emit = defineEmits<{
  select: [value: string]
}>()

function handleClick(suggestion: string) {
  emit('select', suggestion)
}
</script>

<template>
  <div class="suggest-row">
    <button
      v-for="item in suggestions"
      :key="item"
      class="suggest-chip"
      @click="handleClick(item)"
    >
      {{ item }}
    </button>
  </div>
</template>

<style scoped>
.suggest-row {
  margin-top: 10px;
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.suggest-chip {
  border-radius: 999px;
  border: 1px solid #e5e7eb;
  background: #ffffff;
  padding: 6px 10px;
  font-size: 12px;
  color: #374151;
  cursor: pointer;
  transition:
    background 0.16s ease,
    box-shadow 0.16s ease,
    transform 0.1s ease;
}

.dark .suggest-chip {
  background: #374151;
  border-color: #4b5563;
  color: #d1d5db;
}

.suggest-chip:hover {
  background: #f3f4f6;
  box-shadow: 0 4px 10px rgba(148, 163, 184, 0.35);
  transform: translateY(-1px);
}

.dark .suggest-chip:hover {
  background: #4b5563;
}
</style>



