import { useEffect, useState } from 'react';
import { Avatar } from 'antd';
import { UserOutlined } from '@ant-design/icons';
import { api, resolveApiUrl } from './api';
import type { UserProfile } from './types';

// 模块级缓存：头像/用户名变化频率极低（换头像才会变），全站每 uid 只拉一次；
// 404（该 uid 尚未同步过，后端没有其资料）与其它错误同样静默回退 null
const cache = new Map<string, UserProfile | null>();
const inflight = new Map<string, Promise<UserProfile | null>>();

function loadProfile(uid: string): Promise<UserProfile | null> {
  if (cache.has(uid)) return Promise.resolve(cache.get(uid)!);
  let p = inflight.get(uid);
  if (!p) {
    p = api
      .getUserProfile(uid)
      .catch(() => null)
      .then((v) => {
        cache.set(uid, v);
        inflight.delete(uid);
        return v;
      });
    inflight.set(uid, p);
  }
  return p;
}

/** 用户资料（头像 + 用户名）；uid 为空或后端尚未同步过该用户时为 null */
export function useUserProfile(uid: string | null | undefined): UserProfile | null {
  const [profile, setProfile] = useState<UserProfile | null>(() =>
    uid ? (cache.get(uid) ?? null) : null,
  );
  useEffect(() => {
    if (!uid) {
      setProfile(null);
      return;
    }
    let alive = true;
    loadProfile(uid).then((p) => {
      if (alive) setProfile(p);
    });
    return () => {
      alive = false;
    };
  }, [uid]);
  return profile;
}

/** 用户头像图：经本地封面代理加载（豆瓣头像域浏览器直连 418），无资料时回退图标 */
export function UserAvatar({ uid, size = 24 }: { uid: string | null | undefined; size?: number }) {
  const profile = useUserProfile(uid);
  return (
    <Avatar
      size={size}
      src={profile?.avatar ? resolveApiUrl(profile.avatar) : undefined}
      icon={<UserOutlined />}
    >
      {profile?.username ? profile.username.slice(0, 1) : undefined}
    </Avatar>
  );
}
