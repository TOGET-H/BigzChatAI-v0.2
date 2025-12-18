<script setup lang="ts">
import { ref, watch } from 'vue'
import { useChatStore } from '@/stores/chat'

const chatStore = useChatStore()

const input = ref('')
const textareaRef = ref<HTMLTextAreaElement>()

// 自动调整 textarea 高度
function adjustHeight() {
  if (textareaRef.value) {
    textareaRef.value.style.height = 'auto'
    textareaRef.value.style.height = `${textareaRef.value.scrollHeight}px`
  }
}

watch(input, adjustHeight)

function handleSend() {
  const text = input.value.trim()
  if (!text || chatStore.isLoading) return

  chatStore.sendMessage(text)
  input.value = ''
  if (textareaRef.value) {
    textareaRef.value.style.height = 'auto'
  }
}

function handleKeyDown(event: KeyboardEvent) {
  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault()
    handleSend()
  }
}

// 设置输入值（供外部调用）
function setValue(value: string) {
  input.value = value
  adjustHeight()
  textareaRef.value?.focus()
}

defineExpose({
  setValue,
})
</script>

<template>
  <div class="chat-input">
    <textarea
      ref="textareaRef"
      v-model="input"
      class="chat-input__textarea"
      rows="1"
      placeholder="请输入你的问题或想法，按 Enter 发送..."
      @keydown="handleKeyDown"
    ></textarea>
    <div class="chat-input__footer">
      <span class="chat-input__hint">按 Enter 发送 · Shift + Enter 换行</span>
      <button
        class="send-btn"
        :class="{ 'send-btn--disabled': !input.trim() || chatStore.isLoading }"
        @click="handleSend"
      >
        <span v-if="!chatStore.isLoading">发送</span>
        <span v-else>思考中…</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.chat-input {
  margin-top: 8px;
  border-radius: 14px;
  border: 1px solid #e5e7eb;
  background: #f9fafb;
  padding: 8px 10px 6px;
}

.dark .chat-input {
  background: #374151;
  border-color: #4b5563;
}

.chat-input__textarea {
  width: 100%;
  border: none;
  outline: none;
  resize: none;
  background: transparent;
  color: #111827;
  font-size: 13px;
  line-height: 1.5;
  max-height: 200px;
  overflow-y: auto;
}

.dark .chat-input__textarea {
  color: #e5e7eb;
}

.chat-input__textarea::placeholder {
  color: #9ca3af;
}

.chat-input__footer {
  margin-top: 4px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.chat-input__hint {
  font-size: 11px;
  color: #9ca3af;
}

.send-btn {
  border-radius: 999px;
  padding: 5px 14px;
  border: none;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  background: #111827;
  color: #f9fafb;
  box-shadow:
    0 10px 20px rgba(15, 23, 42, 0.25),
    0 0 0 1px rgba(15, 23, 42, 0.85);
  transition:
    background 0.14s ease,
    box-shadow 0.16s ease,
    transform 0.1s ease,
    opacity 0.12s ease;
}

.dark .send-btn {
  background: #4b5563;
}

.send-btn--disabled {
  opacity: 0.4;
  cursor: default;
  box-shadow: none;
}

.send-btn:not(.send-btn--disabled):hover {
  background: #0f172a;
  transform: translateY(-1px);
  box-shadow:
    0 14px 26px rgba(15, 23, 42, 0.35),
    0 0 0 1px rgba(15, 23, 42, 0.9);
}

.dark .send-btn:not(.send-btn--disabled):hover {
  background: #6b7280;
}

@media (max-width: 768px) {
  .chat-input__footer {
    flex-direction: column;
    align-items: flex-end;
  }
}
</style>

