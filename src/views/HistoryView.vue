<script setup lang="ts">
import { onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { useChatStore } from '@/stores/chat'
import ChatHeader from '@/components/ChatHeader.vue'
import { formatRelativeTime } from '@/utils/format'

const router = useRouter()
const chatStore = useChatStore()

const conversations = computed(() => chatStore.conversations)

function handleConversationClick(id: string) {
  chatStore.loadConversation(id)
  router.push('/')
}

function handleDelete(id: string, event: Event) {
  event.stopPropagation()
  if (confirm('确定要删除这个对话吗？')) {
    chatStore.deleteConversation(id)
  }
}

function handleNewChat() {
  chatStore.newConversation()
  router.push('/')
}

onMounted(() => {
  chatStore.initStore()
})
</script>

<template>
  <div class="page">
    <ChatHeader />

    <main class="history-main">
      <div class="history-container">
        <div class="history-header">
          <h1>对话历史</h1>
          <button class="new-chat-btn" @click="handleNewChat">新建对话</button>
        </div>

        <div v-if="conversations.length === 0" class="empty-state">
          <p>还没有对话记录</p>
          <button class="new-chat-btn" @click="handleNewChat">开始第一个对话</button>
        </div>

        <div v-else class="conversation-list">
          <div
            v-for="conversation in conversations"
            :key="conversation.id"
            class="conversation-item"
            @click="handleConversationClick(conversation.id)"
          >
            <div class="conversation-content">
              <h3 class="conversation-title">{{ conversation.title }}</h3>
              <p class="conversation-meta">
                {{ conversation.messages.length }} 条消息 · {{ formatRelativeTime(conversation.updatedAt) }}
              </p>
            </div>
            <button
              class="delete-btn"
              @click="handleDelete(conversation.id, $event)"
            >
              删除
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

.history-main {
  flex: 1;
  display: flex;
  justify-content: center;
  padding: 24px 16px 32px;
}

.history-container {
  width: 100%;
  max-width: 960px;
}

.history-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 24px;
}

.history-header h1 {
  font-size: 24px;
  font-weight: 600;
}

.new-chat-btn {
  border-radius: 999px;
  padding: 8px 16px;
  border: 1px solid #e5e7eb;
  background: #111827;
  color: #f9fafb;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition:
    background 0.16s ease,
    transform 0.1s ease;
}

.dark .new-chat-btn {
  background: #374151;
  border-color: #4b5563;
}

.new-chat-btn:hover {
  background: #0f172a;
  transform: translateY(-1px);
}

.dark .new-chat-btn:hover {
  background: #4b5563;
}

.empty-state {
  text-align: center;
  padding: 60px 20px;
  color: #6b7280;
}

.dark .empty-state {
  color: #9ca3af;
}

.empty-state p {
  font-size: 16px;
  margin-bottom: 16px;
}

.conversation-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.conversation-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px;
  background: #ffffff;
  border: 1px solid #e5e7eb;
  border-radius: 12px;
  cursor: pointer;
  transition:
    background 0.16s ease,
    box-shadow 0.16s ease,
    transform 0.1s ease;
}

.dark .conversation-item {
  background: #1f2937;
  border-color: #374151;
}

.conversation-item:hover {
  background: #f9fafb;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
  transform: translateY(-1px);
}

.dark .conversation-item:hover {
  background: #374151;
}

.conversation-content {
  flex: 1;
}

.conversation-title {
  font-size: 15px;
  font-weight: 500;
  margin-bottom: 4px;
}

.conversation-meta {
  font-size: 12px;
  color: #6b7280;
}

.dark .conversation-meta {
  color: #9ca3af;
}

.delete-btn {
  padding: 6px 12px;
  border: 1px solid #e5e7eb;
  background: transparent;
  color: #ef4444;
  font-size: 12px;
  border-radius: 6px;
  cursor: pointer;
  transition:
    background 0.16s ease,
    border-color 0.16s ease;
}

.dark .delete-btn {
  border-color: #4b5563;
}

.delete-btn:hover {
  background: #fef2f2;
  border-color: #ef4444;
}

.dark .delete-btn:hover {
  background: #7f1d1d;
}

@media (max-width: 768px) {
  .history-main {
    padding: 16px 12px 24px;
  }

  .history-header {
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
  }

  .new-chat-btn {
    width: 100%;
  }
}
</style>



