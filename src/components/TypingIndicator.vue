<script setup lang="ts"></script>

<template>
  <div class="chat-bubble chat-bubble--assistant chat-bubble--thinking">
    <div class="chat-bubble__avatar">
      <span>AI</span>
    </div>
    <div class="chat-bubble__content">
      <div class="chat-bubble__header">
        <span class="chat-bubble__role">BigzChat AI</span>
      </div>
      <div class="typing">
        <span class="typing__dot" />
        <span class="typing__dot" />
        <span class="typing__dot" />
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
  background: #e5f3ff;
  color: #1d4ed8;
  flex-shrink: 0;
}

.dark .chat-bubble__avatar {
  background: #1e3a8a;
  color: #93c5fd;
}

.chat-bubble__content {
  max-width: calc(100% - 40px);
  border-radius: 14px;
  padding: 8px 10px;
  background: #f3f4f6;
  border: 1px dashed #d1d5db;
}

.dark .chat-bubble__content {
  background: #374151;
  border-color: #4b5563;
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

.typing {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.typing__dot {
  width: 6px;
  height: 6px;
  border-radius: 999px;
  background: #d1d5db;
  animation: typing-bounce 1.1s infinite ease-in-out;
}

.dark .typing__dot {
  background: #6b7280;
}

.typing__dot:nth-child(2) {
  animation-delay: 0.15s;
}

.typing__dot:nth-child(3) {
  animation-delay: 0.3s;
}

@keyframes typing-bounce {
  0%,
  80%,
  100% {
    transform: translateY(0);
    opacity: 0.5;
  }
  40% {
    transform: translateY(-4px);
    opacity: 1;
  }
}
</style>



