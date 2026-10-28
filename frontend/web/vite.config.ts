import react, { reactCompilerPreset } from '@vitejs/plugin-react'
import babel from '@rolldown/plugin-babel'
import { defineConfig } from 'vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    babel({ presets: [reactCompilerPreset()] })
  ],
  server: {
    // 开发期把后端 API 代理到本地同步服务器（用户资料 / marks / 封面代理），
    // 部署时由反向代理承担同源转发
    proxy: {
      '/api': { target: 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },
})
