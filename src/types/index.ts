/**
 * 全局类型定义
 */

export * from './chat'

export interface ApiResponse<T = any> {
  code: number
  message: string
  data: T
}

export interface ApiError {
  code: number
  message: string
  details?: any
}



