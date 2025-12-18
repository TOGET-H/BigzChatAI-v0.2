/**
 * 表单验证工具函数
 */

/**
 * 验证邮箱格式
 */
export function isValidEmail(email: string): boolean {
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/
  return emailRegex.test(email)
}

/**
 * 验证非空字符串
 */
export function isNotEmpty(value: string): boolean {
  return value.trim().length > 0
}

/**
 * 验证最小长度
 */
export function minLength(value: string, min: number): boolean {
  return value.length >= min
}

/**
 * 验证最大长度
 */
export function maxLength(value: string, max: number): boolean {
  return value.length <= max
}



