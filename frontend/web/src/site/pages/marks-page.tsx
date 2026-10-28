import { Navigate, useParams } from 'react-router'

import {
  DEFAULT_MARKS_PATH,
  MARKS_STATUS,
  resolveMarksView,
} from '../navigation'
import './marks-page.css'

/** 标记列表页骨架：路由参数驱动标题，列表内容待接入 /api/v1/users/{uid}/marks */
export function MarksPage() {
  const { category } = useParams()
  const view = resolveMarksView(category)

  if (!view) {
    return <Navigate replace to={DEFAULT_MARKS_PATH} />
  }

  return (
    <section className="marks-page">
      <h1 className="marks-page__title">{view.pageTitle}</h1>
      <div className="marks-page__placeholder">
        <p>列表内容建设中，将展示 {view.pageTitle} 的条目。</p>
        <p>
          数据源：GET /api/v1/users/&#123;uid&#125;/marks?category=
          {view.category}&amp;status={MARKS_STATUS}
        </p>
      </div>
    </section>
  )
}
