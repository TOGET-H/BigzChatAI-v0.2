<script setup lang="ts">
import { RouterView } from 'vue-router'
import { onMounted } from 'vue'
import { useThemeStore } from '@/stores/theme'

const themeStore = useThemeStore()

onMounted(() => {
  themeStore.initTheme()
})
</script>

<template>
  <RouterView />
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

.model-select {
  border: none;
  background: transparent;
  font-size: 12px;
  color: #111827;
  padding-right: 4px;
  outline: none;
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

.header-button:hover {
  background: #f3f4f6;
  box-shadow: 0 4px 10px rgba(148, 163, 184, 0.35);
  transform: translateY(-1px);
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

.suggest-chip:hover {
  background: #f3f4f6;
  box-shadow: 0 4px 10px rgba(148, 163, 184, 0.35);
  transform: translateY(-1px);
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
}

.chat-bubble--user .chat-bubble__avatar {
  background: #111827;
  color: #f9fafb;
}

.chat-bubble--assistant .chat-bubble__avatar {
  background: #e5f3ff;
  color: #1d4ed8;
}

.chat-bubble__content {
  max-width: calc(100% - 40px);
  border-radius: 14px;
  padding: 8px 10px;
}

.chat-bubble--user .chat-bubble__content {
  margin-left: auto;
  background: #111827;
  color: #f9fafb;
}

.chat-bubble--assistant .chat-bubble__content {
  background: #f3f4f6;
  color: #111827;
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
}

.chat-bubble__text {
  font-size: 13px;
  line-height: 1.6;
  white-space: pre-wrap;
}

.chat-bubble--thinking .chat-bubble__content {
  border: 1px dashed #d1d5db;
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

.chat-input {
  margin-top: 8px;
  border-radius: 14px;
  border: 1px solid #e5e7eb;
  background: #f9fafb;
  padding: 8px 10px 6px;
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

  .app-main {
    padding: 16px 12px 24px;
  }

  .chat-card {
    padding: 10px 8px 8px;
  }

  .chat-stream {
    max-height: calc(100vh - 260px);
  }

  .chat-input__footer {
    flex-direction: column;
    align-items: flex-end;
  }
}
</style>
