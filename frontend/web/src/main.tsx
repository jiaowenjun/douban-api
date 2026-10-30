import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClientProvider } from '@tanstack/react-query'
import './index.css'
import App from './App.tsx'
import { queryClient } from './query-client.ts'
import { restoreQueryCache, startQueryPersistence } from './query-persistence'

// Storage can be blocked by another tab: do not hold the application indefinitely.
const restoration = new AbortController()
await Promise.race([restoreQueryCache(restoration.signal), new Promise((resolve) => setTimeout(resolve, 1000))])
restoration.abort()
startQueryPersistence()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
)
