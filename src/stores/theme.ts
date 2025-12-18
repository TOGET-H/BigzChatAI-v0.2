import { ref, watch } from 'vue'
import { defineStore } from 'pinia'
import { getStorage, setStorage } from '@/utils/storage'

export type Theme = 'light' | 'dark'

export const useThemeStore = defineStore('theme', () => {
  const theme = ref<Theme>((getStorage<Theme>('theme', 'light') || 'light') as Theme)

  // 初始化主题
  function initTheme() {
    applyTheme(theme.value)
  }

  // 切换主题
  function toggleTheme() {
    theme.value = theme.value === 'light' ? 'dark' : 'light'
  }

  // 设置主题
  function setTheme(newTheme: Theme) {
    theme.value = newTheme
  }

  // 应用主题到 DOM
  function applyTheme(newTheme: Theme) {
    const root = document.documentElement
    if (newTheme === 'dark') {
      root.classList.add('dark')
    } else {
      root.classList.remove('dark')
    }
  }

  // 监听主题变化
  watch(
    theme,
    (newTheme) => {
      applyTheme(newTheme)
      setStorage('theme', newTheme)
    },
    { immediate: true }
  )

  return {
    theme,
    initTheme,
    toggleTheme,
    setTheme,
  }
})



