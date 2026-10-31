import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  DEFAULT_SITE_SETTINGS,
  parseStoredSiteSettings,
} from '../src/site/site-settings.ts'

test('parseStoredSiteSettings 回落默认：空值/坏 JSON/非对象', () => {
  assert.deepEqual(parseStoredSiteSettings(null), DEFAULT_SITE_SETTINGS)
  assert.deepEqual(parseStoredSiteSettings(''), DEFAULT_SITE_SETTINGS)
  assert.deepEqual(parseStoredSiteSettings('{oops'), DEFAULT_SITE_SETTINGS)
  assert.deepEqual(parseStoredSiteSettings('true'), DEFAULT_SITE_SETTINGS)
  assert.deepEqual(parseStoredSiteSettings('[]'), DEFAULT_SITE_SETTINGS)
})

test('parseStoredSiteSettings 字段级容错：非法字段回落默认、合法字段保留', () => {
  // defaultMetaExpanded 类型非法：仅该字段回落，其余合法字段保留
  assert.deepEqual(
    parseStoredSiteSettings('{"defaultMetaExpanded":"yes","collapsedTitleAlign":"left"}'),
    { defaultMetaExpanded: false, collapsedTitleAlign: 'left' },
  )
  // collapsedTitleAlign 取值非法（不在枚举内）：仅该字段回落
  assert.deepEqual(
    parseStoredSiteSettings('{"defaultMetaExpanded":true,"collapsedTitleAlign":"right"}'),
    { defaultMetaExpanded: true, collapsedTitleAlign: 'center' },
  )
  // 旧版本存档缺新字段：缺省字段回落默认，不废弃整个存档
  assert.deepEqual(parseStoredSiteSettings('{"defaultMetaExpanded":true}'), {
    defaultMetaExpanded: true,
    collapsedTitleAlign: 'center',
  })
})

test('parseStoredSiteSettings 合法存档透传，未知字段丢弃', () => {
  assert.deepEqual(
    parseStoredSiteSettings('{"defaultMetaExpanded":true,"collapsedTitleAlign":"left"}'),
    { defaultMetaExpanded: true, collapsedTitleAlign: 'left' },
  )
  assert.deepEqual(
    parseStoredSiteSettings('{"defaultMetaExpanded":false,"collapsedTitleAlign":"center","legacy":2}'),
    { defaultMetaExpanded: false, collapsedTitleAlign: 'center' },
  )
})
