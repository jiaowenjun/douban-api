import { dehydrate, hydrate } from '@tanstack/react-query'
import type { DehydratedState } from '@tanstack/react-query'
import { queryClient, DOUBAN_CACHE_MAX_AGE_MS } from './query-client.ts'

const DATABASE = 'weimo-douban-cache-v1'
const STORE = 'queries'
const MAX_BYTES = 8 * 1024 * 1024
const MAX_QUERIES = 150
let epoch = 0
let pending: ReturnType<typeof setTimeout> | undefined
let database: Promise<IDBDatabase> | undefined
let writes = Promise.resolve()

function openDatabase(): Promise<IDBDatabase> {
  database ??= new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE, 1)
    request.onupgradeneeded = () => request.result.createObjectStore(STORE)
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
    request.onblocked = () => reject(new Error('缓存数据库被占用'))
  })
  return database
}

async function write(snapshot: DehydratedState | null, capturedEpoch: number) {
  const db = await openDatabase()
  if (epoch !== capturedEpoch) return
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE, 'readwrite')
    const store = tx.objectStore(STORE)
    if (snapshot) store.put(snapshot, location.origin)
    else store.delete(location.origin)
    tx.oncomplete = () => resolve()
    tx.onerror = () => reject(tx.error)
    tx.onabort = () => reject(tx.error)
  })
}

function snapshot(): DehydratedState {
  const result = dehydrate(queryClient, {
    shouldDehydrateQuery: (query) => query.queryKey[0] === 'douban' &&
      query.state.dataUpdatedAt > 0 && query.state.data != null &&
      !(typeof query.state.data === 'object' && 'ok' in query.state.data && query.state.data.ok === false) &&
      Date.now() - query.state.dataUpdatedAt < DOUBAN_CACHE_MAX_AGE_MS,
  })
  // A background error still owns a valid successful payload. Hydrate it as usable
  // data; transient error objects and fetch state never belong in durable storage.
  result.queries = result.queries.map((query) => ({
    ...query,
    state: { ...query.state, status: 'success', error: null, fetchStatus: 'idle', fetchFailureCount: 0, fetchFailureReason: null },
  }))
  let bytes = 0
  result.queries = result.queries.sort((a, b) => b.state.dataUpdatedAt - a.state.dataUpdatedAt)
    .filter((query, index) => {
      bytes += new TextEncoder().encode(JSON.stringify(query)).length
      return index < MAX_QUERIES && bytes <= MAX_BYTES
    })
  return result
}

/** Hydrate before React mounts so observers never issue a redundant cold request.
 * Storage failure is optional: the normal in-memory query cache remains available. */
export async function restoreQueryCache(signal?: AbortSignal): Promise<void> {
  try {
    const db = await openDatabase()
    const stored = await new Promise<DehydratedState | undefined>((resolve, reject) => {
      const request = db.transaction(STORE).objectStore(STORE).get(location.origin)
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
    if (!signal?.aborted && stored && Array.isArray(stored.queries)) {
      stored.queries = stored.queries.filter((q) => q.queryKey?.[0] === 'douban' &&
        q.state.status === 'success' && Date.now() - q.state.dataUpdatedAt < DOUBAN_CACHE_MAX_AGE_MS)
      hydrate(queryClient, stored)
    }
  } catch { /* private mode / quota / unavailable storage: use memory */ }
}

export function startQueryPersistence() {
  return queryClient.getQueryCache().subscribe((event) => {
    if (event.type !== 'removed' && !(event.type === 'updated' && event.action.type === 'success')) return
    clearTimeout(pending)
    pending = setTimeout(() => {
      const capturedEpoch = epoch
      const data = snapshot()
      writes = writes.then(() => write(data, capturedEpoch)).catch(() => {})
    }, 250)
  })
}

/** Logout/switch invalidates pending writes as well as persisted data. */
export function clearPersistedQueryCache() {
  epoch += 1
  clearTimeout(pending)
  const capturedEpoch = epoch
  writes = writes.then(() => write(null, capturedEpoch)).catch(() => {})
}
