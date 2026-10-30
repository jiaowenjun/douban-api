import 'fake-indexeddb/auto'
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { setTimeout as delay } from 'node:timers/promises'
import { queryClient, DOUBAN_CACHE_MAX_AGE_MS } from '../src/query-client.ts'
import { clearPersistedQueryCache, restoreQueryCache, startQueryPersistence } from '../src/query-persistence.ts'

test('persistent cache restores only successful recent queries and survives a failed refresh', async () => {
  Object.defineProperty(globalThis, 'location', { value: { origin: 'http://cache.test' }, configurable: true })
  const key = ['douban', 'alice', 'marks', 'movie', 'revision-1']
  const cached = { ok: true, data: { items: [{ subject_id: '1' }] } }
  const stop = startQueryPersistence()
  queryClient.setQueryData(key, cached)
  queryClient.setQueryData(['douban', 'alice', 'error'], { ok: false, kind: 'error' })
  queryClient.setQueryData(['douban', 'alice', 'missing'], null)
  queryClient.setQueryData(['douban', 'alice', 'expired'], { value: 1 }, { updatedAt: Date.now() - DOUBAN_CACHE_MAX_AGE_MS - 1 })
  queryClient.setQueryData(['another-app'], { value: 1 })
  await delay(400)
  stop()
  queryClient.clear()
  await restoreQueryCache()
  assert.deepEqual(queryClient.getQueryData(key), cached)
  assert.equal(queryClient.getQueryCache().getAll().length, 1)
  let calls = 0
  await queryClient.fetchQuery({ queryKey: key, queryFn: async () => { calls++; return cached }, staleTime: 60_000 })
  assert.equal(calls, 0, 'restored data must not cause a cold request')
  await assert.rejects(queryClient.fetchQuery({ queryKey: key, queryFn: async () => { throw new Error('offline') }, retry: false }))
  assert.deepEqual(queryClient.getQueryData(key), cached, 'failed refresh preserves successful data')
  const stopAfterFailure = startQueryPersistence()
  queryClient.setQueryData(['douban', 'alice', 'versions'], { all: 'v1' })
  await delay(400)
  stopAfterFailure()
  queryClient.clear()
  await restoreQueryCache()
  assert.deepEqual(queryClient.getQueryData(key), cached, 'disk keeps last success after a failed refresh')
  assert.equal(queryClient.getQueryState(key)?.status, 'success')
  clearPersistedQueryCache()
  await delay(100)
  queryClient.clear()
  await restoreQueryCache()
  assert.equal(queryClient.getQueryCache().getAll().length, 0)
})

test('logout cancels a delayed persistence write; aborted restore cannot bring it back', async () => {
  const stop = startQueryPersistence()
  queryClient.setQueryData(['douban', 'alice', 'versions'], { all: 'v1' })
  clearPersistedQueryCache()
  queryClient.clear()
  await delay(400)
  stop()
  const abort = new AbortController()
  abort.abort()
  await restoreQueryCache(abort.signal)
  assert.equal(queryClient.getQueryCache().getAll().length, 0)
})
