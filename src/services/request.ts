/**
 * HTTP 请求封装
 */

import type { ApiResponse, ApiError } from '@/types'

// 请求配置
const BASE_URL = import.meta.env.VITE_API_BASE_URL || '/api'
const TIMEOUT = 30000

// 请求拦截器
function requestInterceptor(config: RequestInit): RequestInit {
  // 添加认证 token（如果有）
  const token = localStorage.getItem('token')
  const headers = new Headers(config.headers)

  if (token) {
    headers.set('Authorization', `Bearer ${token}`)
  }

  headers.set('Content-Type', 'application/json')

  return {
    ...config,
    headers,
  }
}

// 响应拦截器
async function responseInterceptor<T>(response: Response): Promise<T> {
  if (!response.ok) {
    const error: ApiError = {
      code: response.status,
      message: response.statusText || '请求失败',
    }

    try {
      const errorData = await response.json()
      if (errorData.message) {
        error.message = errorData.message
      }
      if (errorData.details) {
        error.details = errorData.details
      }
    } catch {
      // 如果响应不是 JSON，使用默认错误信息
    }

    throw error
  }

  const data = await response.json()

  // 如果返回的是 ApiResponse 格式
  if (data && typeof data === 'object' && 'code' in data) {
    if (data.code !== 200 && data.code !== 0) {
      throw {
        code: data.code,
        message: data.message || '请求失败',
        details: data.details,
      } as ApiError
    }
    return data.data as T
  }

  return data as T
}

/**
 * 通用请求方法
 */
export async function request<T = any>(
  url: string,
  options: RequestInit = {}
): Promise<T> {
  const controller = new AbortController()
  const timeoutId = setTimeout(() => controller.abort(), TIMEOUT)

  try {
    const config = requestInterceptor({
      ...options,
      signal: controller.signal,
    })

    const fullUrl = url.startsWith('http') ? url : `${BASE_URL}${url}`
    const response = await fetch(fullUrl, config)

    clearTimeout(timeoutId)
    return await responseInterceptor<T>(response)
  } catch (error: any) {
    clearTimeout(timeoutId)

    if (error.name === 'AbortError') {
      throw {
        code: 408,
        message: '请求超时，请稍后重试',
      } as ApiError
    }

    throw error
  }
}

/**
 * GET 请求
 */
export function get<T = any>(url: string, params?: Record<string, any>): Promise<T> {
  const queryString = params
    ? '?' + new URLSearchParams(params).toString()
    : ''
  return request<T>(url + queryString, {
    method: 'GET',
  })
}

/**
 * POST 请求
 */
export function post<T = any>(url: string, data?: any): Promise<T> {
  return request<T>(url, {
    method: 'POST',
    body: JSON.stringify(data),
  })
}

/**
 * PUT 请求
 */
export function put<T = any>(url: string, data?: any): Promise<T> {
  return request<T>(url, {
    method: 'PUT',
    body: JSON.stringify(data),
  })
}

/**
 * DELETE 请求
 */
export function del<T = any>(url: string): Promise<T> {
  return request<T>(url, {
    method: 'DELETE',
  })
}



