# BigzChat - Vue3 AI 聊天应用

一个基于 Vue 3 + TypeScript 构建的现代化 AI 聊天应用，采用 Composition API、Pinia 状态管理、Vue Router 等最新技术栈。

## ✨ 功能特性

- 💬 **智能对话** - 支持多轮对话，流畅的聊天体验
- 📝 **Markdown 渲染** - 支持 Markdown 格式，代码高亮显示
- 🌓 **主题切换** - 浅色/深色主题，支持系统偏好
- 📚 **对话历史** - 完整的对话历史管理，支持搜索和删除
- 💾 **本地存储** - 对话数据持久化到本地存储
- 📱 **响应式设计** - 完美适配桌面端和移动端
- 🎨 **现代化 UI** - 简洁优雅的用户界面设计
- ⚡ **性能优化** - 组件懒加载、虚拟滚动等优化

## 🛠️ 技术栈

- **Vue 3** - 渐进式 JavaScript 框架
- **TypeScript** - 类型安全的 JavaScript
- **Pinia** - Vue 官方状态管理库
- **Vue Router** - 官方路由管理器
- **Vite** - 下一代前端构建工具
- **Composition API** - Vue 3 组合式 API

## 📁 项目结构

```
src/
├── components/          # 可复用组件
│   ├── ChatHeader.vue      # 顶部导航栏
│   ├── ChatMessage.vue     # 消息气泡组件
│   ├── ChatInput.vue       # 输入框组件
│   ├── TypingIndicator.vue # 打字动画
│   ├── MarkdownRenderer.vue # Markdown 渲染器
│   └── SuggestChips.vue    # 快捷建议按钮
├── views/              # 页面组件
│   ├── ChatView.vue        # 聊天主页面
│   ├── HistoryView.vue     # 历史对话页面
│   ├── SettingsView.vue    # 设置页面
│   └── AboutView.vue       # 关于页面
├── stores/             # Pinia 状态管理
│   ├── chat.ts            # 聊天状态管理
│   └── theme.ts           # 主题状态管理
├── services/           # API 服务层
│   ├── api.ts            # API 接口定义
│   └── request.ts        # HTTP 请求封装
├── utils/              # 工具函数
│   ├── format.ts         # 格式化工具
│   ├── storage.ts        # 本地存储工具
│   ├── debounce.ts       # 防抖节流
│   └── validator.ts      # 表单验证
├── types/              # TypeScript 类型定义
│   ├── chat.ts           # 聊天相关类型
│   └── index.ts          # 全局类型
└── router/             # 路由配置
    └── index.ts
```

## 🚀 快速开始

### 安装依赖

```bash
npm install
```

### 开发模式

```bash
npm run dev
```

### 构建生产版本

```bash
npm run build
```

### 预览生产构建

```bash
npm run preview
```

## 📖 使用说明

### 基本使用

1. 启动项目后，访问 `http://localhost:5173`
2. 在输入框中输入问题，按 Enter 发送
3. 点击顶部导航栏的"历史"查看对话历史
4. 点击"设置"可以切换主题和模型

### 功能说明

- **发送消息**: 在输入框输入内容，按 `Enter` 发送，`Shift + Enter` 换行
- **切换模型**: 在顶部导航栏选择不同的 AI 模型
- **查看历史**: 点击"历史"按钮查看所有对话记录
- **切换主题**: 点击顶部导航栏的月亮/太阳图标切换主题
- **删除对话**: 在历史页面点击"删除"按钮删除对话

## 🔧 配置

### 环境变量

创建 `.env` 文件：

```env
VITE_API_BASE_URL=http://localhost:3000/api
```

### API 配置

在 `src/services/api.ts` 中配置你的后端 API 地址。

## 📝 代码规范

- 使用 TypeScript 进行类型检查
- 遵循 Vue 3 Composition API 最佳实践
- 使用 ESLint 进行代码检查
- 组件采用单文件组件（SFC）格式

## 🎯 核心功能实现

### 状态管理

使用 Pinia 进行状态管理，包括：
- 聊天消息管理
- 对话历史管理
- 主题切换
- 模型选择

### 路由系统

使用 Vue Router 实现多页面导航：
- `/` - 聊天主页面
- `/history` - 对话历史
- `/settings` - 设置页面
- `/about` - 关于页面

### API 服务

封装了统一的 HTTP 请求方法，支持：
- GET/POST/PUT/DELETE 请求
- 请求拦截器（添加 token）
- 响应拦截器（错误处理）
- 请求超时处理

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

## 📄 许可证

MIT License

## 👨‍💻 作者

BigzChat Team

---

**注意**: 这是一个用于学习和展示 Vue 3 技术的项目。在实际生产环境中，请确保：
1. 配置真实的后端 API
2. 添加用户认证和授权
3. 实现数据加密和安全措施
4. 添加错误监控和日志系统
