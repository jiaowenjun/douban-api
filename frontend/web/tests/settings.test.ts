import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  DEFAULT_SITE_SETTINGS,
  parseStoredSiteSettings,
} from '../src/site/site-settings.ts'

test('parseStoredSiteSettings 回落默认：空值/坏 JSON/非法形状', () => {
  assert.deepEqual(parseStoredSiteSettings(null), DEFAULT_SITE_SETTINGS)
  assert.deepEqual(parseStoredSiteSettings(''), DEFAULT_SITE_SETTINGS)
  assert.deepEqual(parseStoredSiteSettings('{oops'), DEFAULT_SITE_SETTINGS)
  // 非 JSON 对象与字段类型不符（存档废弃，整体回落不逐字段抢救）
  assert.deepEqual(parseStoredSiteSettings('true'), DEFAULT_SITE_SETTINGS)
  assert.deepEqual(parseStoredSiteSettings('[]'), DEFAULT_SITE_SETTINGS)
  assert.deepEqual(
    parseStoredSiteSettings('{"defaultMetaExpanded":"yes"}'),
    DEFAULT_SITE_SETTINGS,
  )
  assert.deepEqual(
    parseStoredSiteSettings('{"defaultMetaExpanded":1}'),
    DEFAULT_SITE_SETTINGS,
  )
})

test('parseStoredSiteSettings 合法存档透传，未知字段丢弃', () => {
  assert.deepEqual(parseStoredSiteSettings('{"defaultMetaExpanded":true}'), {
    defaultMetaExpanded: true,
  })
  assert.deepEqual(
    parseStoredSiteSettings('{"defaultMetaExpanded":false,"legacy":2}'),
    { defaultMetaExpanded: false },
  )
})
