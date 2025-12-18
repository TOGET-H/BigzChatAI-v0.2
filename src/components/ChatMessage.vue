<script setup lang="ts">
import type { Message } from '@/types'
import MarkdownRenderer from './MarkdownRenderer.vue'

defineProps<{
  message: Message
}>()
</script>

<template>
  <div
    class="chat-bubble"
    :class="message.role === 'user' ? 'chat-bubble--user' : 'chat-bubble--assistant'"
  >
    <div class="chat-bubble__avatar">
      <span v-if="message.role === 'user'">你</span>
      <span v-else>AI</span>
    </div>
    <div class="chat-bubble__content">
      <div class="chat-bubble__header">
        <span class="chat-bubble__role">
          {{ message.role === 'user' ? '你' : 'BigzChat AI' }}
        </span>
        <span class="chat-bubble__time">{{ message.time }}</span>
      </div>
      <div class="chat-bubble__text">
        <MarkdownRenderer :content="message.content" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.chat-bubble {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  font-size: 13px;
}

.chat-bubble__avatar {
  width: 26px;
  height: 26px;
  border-radius: 999px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  font-weight: 600;
  flex-shrink: 0;
}

.chat-bubble--user .chat-bubble__avatar {
  background: #111827;
  color: #f9fafb;
}

.dark .chat-bubble--user .chat-bubble__avatar {
  background: #4b5563;
}

.chat-bubble--assistant .chat-bubble__avatar {
  background: #e5f3ff;
  color: #1d4ed8;
}

.dark .chat-bubble--assistant .chat-bubble__avatar {
  background: #1e3a8a;
  color: #93c5fd;
}

.chat-bubble__content {
  max-width: calc(100% - 40px);
  border-radius: 14px;
  padding: 8px 10px;
  word-wrap: break-word;
}

.chat-bubble--user .chat-bubble__content {
  margin-left: auto;
  background: #111827;
  color: #f9fafb;
}

.dark .chat-bubble--user .chat-bubble__content {
  background: #374151;
}

.chat-bubble--assistant .chat-bubble__content {
  background: #f3f4f6;
  color: #111827;
}

.dark .chat-bubble--assistant .chat-bubble__content {
  background: #374151;
  color: #e5e7eb;
}

.chat-bubble__header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 6px;
  margin-bottom: 4px;
}

.chat-bubble__role {
  font-size: 12px;
  font-weight: 500;
}

.chat-bubble__time {
  font-size: 11px;
  color: #9ca3af;
  opacity: 0.7;
}

.chat-bubble__text {
  font-size: 13px;
  line-height: 1.6;
  white-space: pre-wrap;
}

.chat-bubble__text :deep(pre) {
  background: rgba(0, 0, 0, 0.1);
  padding: 8px;
  border-radius: 6px;
  overflow-x: auto;
  margin: 8px 0;
}

.dark .chat-bubble__text :deep(pre) {
  background: rgba(255, 255, 255, 0.1);
}

.chat-bubble__text :deep(code) {
  background: rgba(0, 0, 0, 0.1);
  padding: 2px 4px;
  border-radius: 3px;
  font-size: 12px;
}

.dark .chat-bubble__text :deep(code) {
  background: rgba(255, 255, 255, 0.1);
}

.chat-bubble__text :deep(ul),
.chat-bubble__text :deep(ol) {
  margin: 8px 0;
  padding-left: 20px;
}

.chat-bubble__text :deep(li) {
  margin: 4px 0;
}
</style>

