<script setup lang="ts">
defineProps<{
  message: string
}>()
</script>

<template>
  <div v-if="message" class="error-message">
    <span class="error-icon">⚠️</span>
    <span class="error-text">{{ message }}</span>
  </div>
</template>

<style scoped>
.error-message {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  background: #fef2f2;
  border: 1px solid #fecaca;
  border-radius: 8px;
  color: #dc2626;
  font-size: 13px;
  margin: 8px 0;
}

.dark .error-message {
  background: #7f1d1d;
  border-color: #991b1b;
  color: #fca5a5;
}

.error-icon {
  font-size: 16px;
}

.error-text {
  flex: 1;
}
</style>



