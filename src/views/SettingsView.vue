<script setup lang="ts">
import { onMounted } from 'vue'
import { useChatStore } from '@/stores/chat'
import { useThemeStore } from '@/stores/theme'
import ChatHeader from '@/components/ChatHeader.vue'

const chatStore = useChatStore()
const themeStore = useThemeStore()

const models = ['Bigz GPT-4.5', 'Bigz GPT-4.5 Mini', 'Bigz GPT-Vision']

function handleModelChange(event: Event) {
  const target = event.target as HTMLSelectElement
  chatStore.setModel(target.value)
}

function handleClearHistory() {
  if (confirm('确定要清空所有对话历史吗？此操作不可恢复。')) {
    chatStore.clearChat()
    // 清空所有对话
    chatStore.conversations.splice(0)
    chatStore.saveConversations()
    alert('已清空所有对话历史')
  }
}

onMounted(() => {
  chatStore.initStore()
  themeStore.initTheme()
})
</script>

<template>
  <div class="page">
    <ChatHeader />

    <main class="settings-main">
      <div class="settings-container">
        <h1>设置</h1>

        <div class="settings-section">
          <h2>模型设置</h2>
          <div class="setting-item">
            <label>默认模型</label>
            <select :value="chatStore.currentModel" @change="handleModelChange">
              <option v-for="model in models" :key="model" :value="model">
                {{ model }}
              </option>
            </select>
          </div>
        </div>

        <div class="settings-section">
          <h2>外观设置</h2>
          <div class="setting-item">
            <label>主题</label>
            <div class="theme-toggle">
              <button
                class="theme-btn"
                :class="{ active: themeStore.theme === 'light' }"
                @click="themeStore.setTheme('light')"
              >
                浅色
              </button>
              <button
                class="theme-btn"
                :class="{ active: themeStore.theme === 'dark' }"
                @click="themeStore.setTheme('dark')"
              >
                深色
              </button>
            </div>
          </div>
        </div>

        <div class="settings-section">
          <h2>数据管理</h2>
          <div class="setting-item">
            <label>对话历史</label>
            <button class="danger-btn" @click="handleClearHistory">
              清空所有对话历史
            </button>
          </div>
        </div>
      </div>
    </main>
  </div>
</template>

<style scoped>
.page {
  min-height: 100vh;
  background: #f3f4f6;
  color: #111827;
  font-family: system-ui, -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', sans-serif;
  display: flex;
  flex-direction: column;
}

.dark .page {
  background: #111827;
  color: #e5e7eb;
}

.settings-main {
  flex: 1;
  display: flex;
  justify-content: center;
  padding: 24px 16px 32px;
}

.settings-container {
  width: 100%;
  max-width: 960px;
}

h1 {
  font-size: 24px;
  font-weight: 600;
  margin-bottom: 32px;
}

.settings-section {
  margin-bottom: 32px;
  padding: 24px;
  background: #ffffff;
  border: 1px solid #e5e7eb;
  border-radius: 12px;
}

.dark .settings-section {
  background: #1f2937;
  border-color: #374151;
}

h2 {
  font-size: 18px;
  font-weight: 600;
  margin-bottom: 16px;
}

.setting-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 0;
}

.setting-item label {
  font-size: 14px;
  font-weight: 500;
}

.setting-item select {
  padding: 6px 12px;
  border: 1px solid #e5e7eb;
  border-radius: 6px;
  background: #ffffff;
  font-size: 14px;
  cursor: pointer;
}

.dark .setting-item select {
  background: #374151;
  border-color: #4b5563;
  color: #e5e7eb;
}

.theme-toggle {
  display: flex;
  gap: 8px;
}

.theme-btn {
  padding: 6px 16px;
  border: 1px solid #e5e7eb;
  background: #ffffff;
  border-radius: 6px;
  font-size: 14px;
  cursor: pointer;
  transition:
    background 0.16s ease,
    border-color 0.16s ease;
}

.dark .theme-btn {
  background: #374151;
  border-color: #4b5563;
  color: #e5e7eb;
}

.theme-btn.active {
  background: #111827;
  color: #f9fafb;
  border-color: #111827;
}

.dark .theme-btn.active {
  background: #4b5563;
  border-color: #4b5563;
}

.danger-btn {
  padding: 6px 16px;
  border: 1px solid #ef4444;
  background: transparent;
  color: #ef4444;
  border-radius: 6px;
  font-size: 14px;
  cursor: pointer;
  transition:
    background 0.16s ease,
    color 0.16s ease;
}

.danger-btn:hover {
  background: #ef4444;
  color: #ffffff;
}

@media (max-width: 768px) {
  .settings-main {
    padding: 16px 12px 24px;
  }

  .setting-item {
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }
}
</style>



