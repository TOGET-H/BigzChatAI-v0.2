/**
 * API 服务层 - DeepSeek API 集成
 * 参考文档: https://api-docs.deepseek.com/zh-cn/guides/multi_round_chat
 */

import { post } from './request'
import type { Message, Conversation } from '@/types'
import { DEEPSEEK_API_KEY, DEEPSEEK_API_URL } from '@/utils/env'

// DeepSeek 模型映射
const MODEL_MAP: Record<string, string> = {
  'Bigz GPT-4.5': 'deepseek-chat',
  'Bigz GPT-4.5 Mini': 'deepseek-chat',
  'Bigz GPT-Vision': 'deepseek-chat',
}

export class ChatAPI {
  /**
   * 发送消息（多轮对话）
   * 根据 DeepSeek 文档，需要将完整的对话历史传递给 API
   * @param messages 完整的对话历史（包括之前的所有 user 和 assistant 消息）
   * @param model 模型名称
   */
  static async sendMessage(
    messages: Array<{ role: 'user' | 'assistant'; content: string }>,
    model: string
  ): Promise<{ content: string }> {
    try {
      // 将模型名称映射到 DeepSeek 模型
      const deepseekModel = MODEL_MAP[model] || 'deepseek-chat'

      // 构建请求体
      const requestBody = {
        model: deepseekModel,
        messages: messages,
        temperature: 0.7,
        stream: false,
      }

      // 调用 DeepSeek API
      const response = await fetch(DEEPSEEK_API_URL, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${DEEPSEEK_API_KEY}`,
        },
        body: JSON.stringify(requestBody),
      })

      if (!response.ok) {
        const errorData = await response.json().catch(() => ({}))
        throw new Error(
          errorData.error?.message || `API 请求失败: ${response.status} ${response.statusText}`
        )
      }

      const data = await response.json()

      // 提取回复内容
      const content = data.choices?.[0]?.message?.content

      if (!content) {
        throw new Error('API 返回格式错误：未找到回复内容')
      }

      return { content }
    } catch (error: any) {
      console.error('Send message error:', error)
      throw new Error(error.message || '发送消息失败，请稍后重试')
    }
  }

  /**
   * 获取对话历史
   */
  static async getHistory(): Promise<Conversation[]> {
    try {
      // DeepSeek API 是无状态的，历史记录由客户端管理
      // 这里返回空数组，实际数据从本地存储获取
      return []
    } catch (error) {
      console.error('Get history error:', error)
      throw error
    }
  }

  /**
   * 删除对话
   */
  static async deleteConversation(id: string): Promise<void> {
    try {
      // DeepSeek API 是无状态的，删除操作由客户端管理
      return Promise.resolve()
    } catch (error) {
      console.error('Delete conversation error:', error)
      throw error
    }
  }
}

