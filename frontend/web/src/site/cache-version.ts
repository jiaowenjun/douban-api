import { createContext } from 'react'

export type CacheVersions = {
  profile: string
  stats: string
  movie: string
  book: string
  music: string
  all: string
}
export const CacheVersionContext = createContext<CacheVersions | null | undefined>(undefined)
