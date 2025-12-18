<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { useChatStore } from '@/stores/chat'
import ChatHeader from '@/components/ChatHeader.vue'
import ChatMessage from '@/components/ChatMessage.vue'
import ChatInput from '@/components/ChatInput.vue'
import TypingIndicator from '@/components/TypingIndicator.vue'
import SuggestChips from '@/components/SuggestChips.vue'
import ErrorMessage from '@/components/ErrorMessage.vue'

const chatStore = useChatStore()
const chatStreamRef = ref<HTMLElement>()
const chatInputRef = ref<InstanceType<typeof ChatInput>>()

const quickActions = [
  '帮我规划一个本周学习路线',
  '把这段话润色得更专业一点',
  '帮我写一个产品介绍文案',
  '把下面这段代码重构一下',
]

// 滚动到底部
function scrollToBottom() {
  if (chatStreamRef.value) {
    chatStreamRef.value.scrollTop = chatStreamRef.value.scrollHeight
  }
}

// 监听消息变化，自动滚动
watch(
  () => chatStore.messages.length,
  () => {
    setTimeout(scrollToBottom, 100)
  }
)

// 处理建议选择
function handleSuggestionSelect(text: string) {
  if (chatInputRef.value) {
    chatInputRef.value.setValue(text)
  }
}

onMounted(() => {
  chatStore.initStore()
  scrollToBottom()
})
</script>

<template>
  <div class="page">
    <ChatHeader />

    <main class="app-main">
      <section class="chat-wrapper">
        <div v-if="!chatStore.hasMessages" class="chat-intro">
          <h1>你好，我是 BigzChat AI</h1>
          <p>在这里，用最自然的语言表达你的想法，我会帮你把复杂的事情变得简单。</p>
          <SuggestChips :suggestions="quickActions" @select="handleSuggestionSelect" />
        </div>

        <div class="chat-card">
          <ErrorMessage v-if="chatStore.error" :message="chatStore.error" />
          <div ref="chatStreamRef" class="chat-stream">
            <ChatMessage
              v-for="msg in chatStore.messages"
              :key="msg.id"
              :message="msg"
            />
            <TypingIndicator v-if="chatStore.isLoading" />
          </div>

          <ChatInput ref="chatInputRef" />
        </div>
      </section>
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

.app-main {
  flex: 1;
  display: flex;
  justify-content: center;
  padding: 24px 16px 32px;
}

.chat-wrapper {
  width: 100%;
  max-width: 960px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.chat-intro {
  text-align: left;
  padding: 8px 4px;
}

.chat-intro h1 {
  font-size: 22px;
  font-weight: 600;
  margin-bottom: 4px;
}

.chat-intro p {
  font-size: 13px;
  color: #6b7280;
}

.dark .chat-intro p {
  color: #9ca3af;
}

.chat-card {
  display: flex;
  flex-direction: column;
  border-radius: 18px;
  background: #ffffff;
  border: 1px solid #e5e7eb;
  box-shadow:
    0 18px 40px rgba(15, 23, 42, 0.06),
    0 0 0 1px rgba(148, 163, 184, 0.15);
  padding: 12px 12px 10px;
  min-height: 420px;
}

.dark .chat-card {
  background: #1f2937;
  border-color: #374151;
}

.chat-stream {
  flex: 1;
  min-height: 260px;
  max-height: calc(100vh - 320px);
  padding: 8px 8px 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  overflow-y: auto;
  scrollbar-width: thin;
  scrollbar-color: rgba(156, 163, 175, 0.5) transparent;
}

.chat-stream::-webkit-scrollbar {
  width: 6px;
}

.chat-stream::-webkit-scrollbar-track {
  background: transparent;
}

.chat-stream::-webkit-scrollbar-thumb {
  background: rgba(156, 163, 175, 0.5);
  border-radius: 3px;
}

.dark .chat-stream::-webkit-scrollbar-thumb {
  background: rgba(156, 163, 175, 0.3);
}

@media (max-width: 768px) {
  .app-main {
    padding: 16px 12px 24px;
  }

  .chat-card {
    padding: 10px 8px 8px;
  }

  .chat-stream {
    max-height: calc(100vh - 260px);
  }
}
</style>

