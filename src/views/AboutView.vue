<script setup lang="ts">
import ChatHeader from '@/components/ChatHeader.vue'
</script>

<template>
  <div class="page">
    <ChatHeader />

    <main class="about-main">
      <div class="about-container">
        <h1>关于 BigzChat</h1>

        <div class="about-content">
          <section>
            <h2>项目简介</h2>
            <p>
              BigzChat 是一个基于 Vue 3 + TypeScript 构建的现代化 AI 聊天应用。
              采用 Composition API、Pinia 状态管理、Vue Router 等最新技术栈，
              提供流畅的用户体验和清晰的代码结构。
            </p>
          </section>

          <section>
            <h2>技术栈</h2>
            <ul>
              <li><strong>Vue 3</strong> - 渐进式 JavaScript 框架</li>
              <li><strong>TypeScript</strong> - 类型安全的 JavaScript</li>
              <li><strong>Pinia</strong> - Vue 官方状态管理库</li>
              <li><strong>Vue Router</strong> - 官方路由管理器</li>
              <li><strong>Vite</strong> - 下一代前端构建工具</li>
            </ul>
          </section>

          <section>
            <h2>功能特性</h2>
            <ul>
              <li>✨ 简洁优雅的聊天界面</li>
              <li>💬 支持多轮对话</li>
              <li>📝 Markdown 渲染支持</li>
              <li>🌓 浅色/深色主题切换</li>
              <li>📚 对话历史管理</li>
              <li>💾 本地存储持久化</li>
              <li>📱 响应式设计</li>
            </ul>
          </section>

          <section>
            <h2>项目结构</h2>
            <pre><code>src/
├── components/     # 可复用组件
├── views/          # 页面组件
├── stores/         # Pinia 状态管理
├── services/       # API 服务层
├── utils/          # 工具函数
├── types/          # TypeScript 类型定义
└── router/         # 路由配置</code></pre>
          </section>
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

.about-main {
  flex: 1;
  display: flex;
  justify-content: center;
  padding: 24px 16px 32px;
}

.about-container {
  width: 100%;
  max-width: 960px;
}

h1 {
  font-size: 28px;
  font-weight: 600;
  margin-bottom: 32px;
}

.about-content {
  display: flex;
  flex-direction: column;
  gap: 32px;
}

section {
  padding: 24px;
  background: #ffffff;
  border: 1px solid #e5e7eb;
  border-radius: 12px;
}

.dark section {
  background: #1f2937;
  border-color: #374151;
}

h2 {
  font-size: 20px;
  font-weight: 600;
  margin-bottom: 16px;
}

p {
  font-size: 14px;
  line-height: 1.6;
  color: #6b7280;
}

.dark p {
  color: #9ca3af;
}

ul {
  margin: 0;
  padding-left: 20px;
  font-size: 14px;
  line-height: 1.8;
}

ul li {
  margin: 8px 0;
  color: #6b7280;
}

.dark ul li {
  color: #9ca3af;
}

strong {
  color: #111827;
  font-weight: 600;
}

.dark strong {
  color: #e5e7eb;
}

pre {
  background: #f3f4f6;
  padding: 16px;
  border-radius: 8px;
  overflow-x: auto;
  font-size: 13px;
  line-height: 1.5;
}

.dark pre {
  background: #374151;
}

code {
  font-family: 'Courier New', monospace;
}

@media (max-width: 768px) {
  .about-main {
    padding: 16px 12px 24px;
  }

  section {
    padding: 16px;
  }
}
</style>



