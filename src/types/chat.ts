/**
 * 聊天相关的类型定义
 */

export type Role = 'user' | 'assistant'

export interface Message {
  id: string | number
  role: Role
  content: string
  time: string
  timestamp?: number
}

export interface Conversation {
  id: string
  title: string
  messages: Message[]
  model: string
  createdAt: number
  updatedAt: number
}

export interface ModelConfig {
  id: string
  name: string
  description?: string
  maxTokens?: number
  temperature?: number
}

export interface ChatState {
  messages: Message[]
  currentModel: string
  isLoading: boolean
  error: string | null
  currentConversationId: string | null
}



