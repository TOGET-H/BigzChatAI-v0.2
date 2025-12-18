import { ref, computed } from 'vue'
import { defineStore } from 'pinia'
import type { Message, Conversation } from '@/types'
import { formatTime, generateId } from '@/utils/format'
import { setStorage, getStorage } from '@/utils/storage'
import { ChatAPI } from '@/services/api'

export const useChatStore = defineStore('chat', () => {
  // 状态
  const messages = ref<Message[]>([])
  const currentModel = ref<string>('Bigz GPT-4.5')
  const isLoading = ref(false)
  const error = ref<string | null>(null)
  const currentConversationId = ref<string | null>(null)
  const conversations = ref<Conversation[]>([])

  // 计算属性
  const hasMessages = computed(() => messages.value.length > 0)
  const currentConversation = computed(() => {
    if (!currentConversationId.value) return null
    return conversations.value.find((c) => c.id === currentConversationId.value) || null
  })

  // 初始化：从本地存储加载对话历史
  function initStore() {
    const savedConversations = getStorage<Conversation[]>('conversations', [])
    conversations.value = savedConversations || []

    const savedModel = getStorage<string>('currentModel', 'Bigz GPT-4.5')
    currentModel.value = savedModel || 'Bigz GPT-4.5'
  }

  // 发送消息
  async function sendMessage(content: string) {
    if (!content.trim() || isLoading.value) return

    error.value = null

    // 添加用户消息
    const userMessage: Message = {
      id: generateId(),
      role: 'user',
      content: content.trim(),
      time: formatTime(),
      timestamp: Date.now(),
    }
    messages.value.push(userMessage)

    // 如果没有当前对话，创建新对话
    if (!currentConversationId.value) {
      createNewConversation()
    }

    isLoading.value = true

    try {
      // 构建完整的对话历史（根据 DeepSeek 文档要求，需要传递所有历史消息）
      const conversationHistory = messages.value.map((msg) => ({
        role: msg.role,
        content: msg.content,
      }))

      // 调用 DeepSeek API，传递完整的对话历史
      const response = await ChatAPI.sendMessage(conversationHistory, currentModel.value)

      // 添加 AI 回复
      const assistantMessage: Message = {
        id: generateId(),
        role: 'assistant',
        content: response.content,
        time: formatTime(),
        timestamp: Date.now(),
      }
      messages.value.push(assistantMessage)

      // 更新对话
      if (currentConversationId.value) {
        updateConversation()
      }
    } catch (err: any) {
      error.value = err.message || '发送消息失败，请稍后重试'
      console.error('Send message error:', err)
      
      // 如果出错，移除刚才添加的用户消息（可选，根据需求决定）
      // messages.value.pop()
    } finally {
      isLoading.value = false
    }
  }

  // 创建新对话
  function createNewConversation() {
    const conversation: Conversation = {
      id: generateId(),
      title: messages.value.length > 0
        ? truncate(messages.value[0]?.content ?? '', 30)
        : '新对话',
      messages: [...messages.value],
      model: currentModel.value,
      createdAt: Date.now(),
      updatedAt: Date.now(),
    }

    conversations.value.unshift(conversation)
    currentConversationId.value = conversation.id
    saveConversations()
  }

  // 更新当前对话
  function updateConversation() {
    if (!currentConversationId.value) return

    const conversation = conversations.value.find((c) => c.id === currentConversationId.value)
    if (conversation) {
      conversation.messages = [...messages.value]
      conversation.updatedAt = Date.now()
      conversation.title =
        messages.value.length > 0 && messages.value[0]
          ? truncate(messages.value[0].content, 30)
          : '新对话'
      saveConversations()
    }
  }

  // 加载对话
  function loadConversation(conversationId: string) {
    const conversation = conversations.value.find((c) => c.id === conversationId)
    if (conversation) {
      messages.value = [...conversation.messages]
      currentModel.value = conversation.model
      currentConversationId.value = conversation.id
    }
  }

  // 新建对话
  function newConversation() {
    messages.value = []
    currentConversationId.value = null
  }

  // 删除对话
  function deleteConversation(conversationId: string) {
    const index = conversations.value.findIndex((c) => c.id === conversationId)
    if (index > -1) {
      conversations.value.splice(index, 1)
      saveConversations()

      // 如果删除的是当前对话，清空消息
      if (currentConversationId.value === conversationId) {
        newConversation()
      }
    }
  }

  // 清空当前对话
  function clearChat() {
    messages.value = []
    if (currentConversationId.value) {
      deleteConversation(currentConversationId.value)
    }
  }

  // 设置模型
  function setModel(model: string) {
    currentModel.value = model
    setStorage('currentModel', model)
  }

  // 保存对话列表到本地存储
  function saveConversations() {
    setStorage('conversations', conversations.value)
  }

  // 工具函数：截断文本
  function truncate(text: string, maxLength: number): string {
    if (text.length <= maxLength) return text
    return text.slice(0, maxLength) + '...'
  }

  return {
    // 状态
    messages,
    currentModel,
    isLoading,
    error,
    currentConversationId,
    conversations,
    // 计算属性
    hasMessages,
    currentConversation,
    // 方法
    initStore,
    sendMessage,
    createNewConversation,
    updateConversation,
    loadConversation,
    newConversation,
    deleteConversation,
    clearChat,
    setModel,
    saveConversations,
  }
})

