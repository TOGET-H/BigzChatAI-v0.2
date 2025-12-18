/**
 * 本地存储工具函数
 */

const STORAGE_PREFIX = 'bigzchat_'

/**
 * 获取存储键名
 */
function getKey(key: string): string {
  return `${STORAGE_PREFIX}${key}`
}

/**
 * 设置本地存储
 */
export function setStorage<T>(key: string, value: T): void {
  try {
    const serialized = JSON.stringify(value)
    localStorage.setItem(getKey(key), serialized)
  } catch (error) {
    console.error('Failed to save to localStorage:', error)
  }
}

/**
 * 获取本地存储
 */
export function getStorage<T>(key: string, defaultValue?: T): T | null {
  try {
    const item = localStorage.getItem(getKey(key))
    if (item === null) return defaultValue ?? null
    return JSON.parse(item) as T
  } catch (error) {
    console.error('Failed to read from localStorage:', error)
    return defaultValue ?? null
  }
}

/**
 * 删除本地存储
 */
export function removeStorage(key: string): void {
  try {
    localStorage.removeItem(getKey(key))
  } catch (error) {
    console.error('Failed to remove from localStorage:', error)
  }
}

/**
 * 清空所有应用相关的存储
 */
export function clearStorage(): void {
  try {
    const keys = Object.keys(localStorage)
    keys.forEach((key) => {
      if (key.startsWith(STORAGE_PREFIX)) {
        localStorage.removeItem(key)
      }
    })
  } catch (error) {
    console.error('Failed to clear localStorage:', error)
  }
}



