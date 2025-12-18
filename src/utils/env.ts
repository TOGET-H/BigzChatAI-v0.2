/**
 * 环境变量配置
 * 
 * 环境变量从 .env 文件中读取
 * Vite 要求环境变量必须以 VITE_ 开头才能在前端代码中访问
 */

// DeepSeek API Key - 从环境变量读取，如果没有则使用默认值（仅用于开发）
export const DEEPSEEK_API_KEY =
  import.meta.env.VITE_DEEPSEEK_API_KEY || 'sk-44349f794994429faafce602fac3376b'

// DeepSeek API 地址
export const DEEPSEEK_API_URL = 'https://api.deepseek.com/v1/chat/completions'

// 检查 API Key 是否配置
if (!DEEPSEEK_API_KEY || DEEPSEEK_API_KEY === 'your_deepseek_api_key_here') {
  console.warn(
    '⚠️ DeepSeek API Key 未配置或使用默认值，请在 .env 文件中设置 VITE_DEEPSEEK_API_KEY'
  )
}

