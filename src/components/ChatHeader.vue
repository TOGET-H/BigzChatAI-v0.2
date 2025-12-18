<script setup lang="ts">
import { useRouter } from 'vue-router'
import { useChatStore } from '@/stores/chat'
import { useThemeStore } from '@/stores/theme'

const router = useRouter()
const chatStore = useChatStore()
const themeStore = useThemeStore()

const models = ['Bigz GPT-4.5', 'Bigz GPT-4.5 Mini', 'Bigz GPT-Vision']

function handleModelChange(event: Event) {
  const target = event.target as HTMLSelectElement
  chatStore.setModel(target.value)
}

function goToHistory() {
  router.push('/history')
}

function toggleTheme() {
  themeStore.toggleTheme()
}
</script>

<template>
  <header class="app-header">
    <div class="header-left">
      <div class="logo-dot" />
      <div class="logo-text" @click="router.push('/')" style="cursor: pointer">
        <span class="logo-main">BigzChat</span>
        <span class="logo-sub">轻量 · 简约 · 好用的 AI 聊天</span>
      </div>
    </div>

    <div class="header-center">
      <label class="model-label">
        <span>模型</span>
        <select :value="chatStore.currentModel" class="model-select" @change="handleModelChange">
          <option v-for="model in models" :key="model" :value="model">
            {{ model }}
          </option>
        </select>
      </label>
    </div>

    <div class="header-right">
      <button class="header-button" @click="goToHistory">历史</button>
      <button class="header-button" @click="toggleTheme">
        {{ themeStore.theme === 'light' ? '🌙' : '☀️' }}
      </button>
      <div class="avatar">
        <span>J</span>
      </div>
    </div>
  </header>
</template>

<style scoped>
.app-header {
  position: sticky;
  top: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 24px;
  background: rgba(248, 250, 252, 0.9);
  backdrop-filter: blur(16px);
  border-bottom: 1px solid #e5e7eb;
}

.dark .app-header {
  background: rgba(17, 24, 39, 0.9);
  border-bottom-color: #374151;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 10px;
}

.logo-dot {
  width: 26px;
  height: 26px;
  border-radius: 999px;
  background: radial-gradient(circle at 30% 0%, #22c55e, #3b82f6);
  box-shadow: 0 0 0 1px rgba(148, 163, 184, 0.4);
}

.logo-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.logo-main {
  font-size: 16px;
  font-weight: 600;
  letter-spacing: 0.04em;
}

.logo-sub {
  font-size: 11px;
  color: #6b7280;
}

.header-center {
  flex: 1;
  display: flex;
  justify-content: center;
}

.model-label {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border-radius: 999px;
  background: #f3f4f6;
  border: 1px solid #e5e7eb;
  font-size: 12px;
  color: #6b7280;
}

.dark .model-label {
  background: #374151;
  border-color: #4b5563;
  color: #d1d5db;
}

.model-select {
  border: none;
  background: transparent;
  font-size: 12px;
  color: #111827;
  padding-right: 4px;
  outline: none;
  cursor: pointer;
}

.dark .model-select {
  color: #f9fafb;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.header-button {
  border-radius: 999px;
  padding: 6px 12px;
  border: 1px solid #e5e7eb;
  background: white;
  font-size: 12px;
  color: #374151;
  cursor: pointer;
  transition:
    background 0.16s ease,
    box-shadow 0.16s ease,
    transform 0.1s ease;
}

.dark .header-button {
  background: #374151;
  border-color: #4b5563;
  color: #d1d5db;
}

.header-button:hover {
  background: #f3f4f6;
  box-shadow: 0 4px 10px rgba(148, 163, 184, 0.35);
  transform: translateY(-1px);
}

.dark .header-button:hover {
  background: #4b5563;
}

.avatar {
  width: 28px;
  height: 28px;
  border-radius: 999px;
  background: #111827;
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  font-weight: 600;
}

.dark .avatar {
  background: #4b5563;
}

@media (max-width: 768px) {
  .app-header {
    padding: 10px 16px;
    gap: 8px;
  }

  .logo-sub {
    display: none;
  }

  .header-center {
    display: none;
  }
}
</style>



