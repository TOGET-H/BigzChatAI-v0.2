<script setup lang="ts">
import { ref, onErrorCaptured } from 'vue'

const error = ref<Error | null>(null)

onErrorCaptured((err) => {
  error.value = err
  console.error('Error caught by boundary:', err)
  return false
})

function handleReset() {
  error.value = null
  window.location.reload()
}
</script>

<template>
  <div v-if="error" class="error-boundary">
    <div class="error-content">
      <h2>😔 出现了一些问题</h2>
      <p>{{ error.message }}</p>
      <button class="retry-btn" @click="handleReset">重新加载</button>
    </div>
  </div>
  <slot v-else />
</template>

<style scoped>
.error-boundary {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
  background: #f3f4f6;
}

.dark .error-boundary {
  background: #111827;
}

.error-content {
  text-align: center;
  padding: 40px;
  background: #ffffff;
  border-radius: 12px;
  border: 1px solid #e5e7eb;
  max-width: 500px;
}

.dark .error-content {
  background: #1f2937;
  border-color: #374151;
}

h2 {
  font-size: 24px;
  margin-bottom: 16px;
}

p {
  color: #6b7280;
  margin-bottom: 24px;
}

.dark p {
  color: #9ca3af;
}

.retry-btn {
  padding: 8px 16px;
  background: #111827;
  color: #f9fafb;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
}

.dark .retry-btn {
  background: #374151;
}

.retry-btn:hover {
  background: #0f172a;
}

.dark .retry-btn:hover {
  background: #4b5563;
}
</style>



