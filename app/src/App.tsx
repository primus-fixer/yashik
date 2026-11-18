import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { LockScreen, PinFlow, applyTheme, store, stored, type PinMode, type ThemeMode } from '@primus-fixer/kit/ui';
import { api, LOCKED, message, type LockStatus } from './api';
import { Icon, NAME, NAME_ACC } from './ui';
import Folders from './screens/Folders';
import Inbox from './screens/Inbox';
import Search from './screens/Search';
import Settings from './screens/Settings';

export type Route = { name: 'inbox' } | { name: 'folders' } | { name: 'search' } | { name: 'settings' };

export type Nav = {
  go: (r: Route) => void;
  back: () => void;
  toast: (text: string) => void;
};

/** Разделы: на телефоне — нижняя панель, на компьютере — меню слева. */
const TABS: { id: Route['name']; label: string; icon: string }[] = [
  { id: 'inbox', label: 'Входящие', icon: 'inbox' },
  { id: 'folders', label: 'Папки', icon: 'folder' },
  { id: 'search', label: 'Поиск', icon: 'search' },
  { id: 'settings', label: 'Настройки', icon: 'settings' },
];

function useWide(): boolean {
  const query = '(min-width: 900px)';
  const [wide, setWide] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const mq = window.matchMedia(query);
    const on = () => setWide(mq.matches);
    mq.addEventListener('change', on);
    return () => mq.removeEventListener('change', on);
  }, []);
  return wide;
}

export default function App() {
  const [route, setRoute] = useState<Route>({ name: 'inbox' });
  const [history, setHistory] = useState<Route[]>([]);
  const [toast, showToast] = useState<string | null>(null);
  // Пришли записи с другого устройства: экраны перечитывают данные.
  const [synced, setSynced] = useState(0);
  // Вход по PIN-коду: состояние, экран входа поверх всего, экраны «задать/сменить PIN».
  const [lock, setLock] = useState<LockStatus | null>(null);
  const [locked, setLocked] = useState(false);
  const lockedRef = useRef(false);
  const [lockKey, setLockKey] = useState(0);
  const [pinFlow, setPinFlow] = useState<PinMode | null>(null);
  const [ready, setReady] = useState(false);
  const [scheme, setScheme] = useState(() => stored('yashik.scheme', 'shield'));
  const [mode, setMode] = useState<ThemeMode>(() => stored('yashik.mode', 'system') as ThemeMode);
  const wide = useWide();

  useEffect(() => {
    applyTheme(scheme, mode);
    store('yashik.scheme', scheme);
    store('yashik.mode', mode);
  }, [scheme, mode]);

  /** Экран входа — только если его ещё нет на экране (он сам спросит отпечаток). */
  const showLock = useCallback(() => {
    if (lockedRef.current) return;
    lockedRef.current = true;
    setLockKey((k) => k + 1);
    setLocked(true);
  }, []);
  const goLocked = useCallback(() => {
    api.lockStatus().then(
      (st) => {
        setLock(st);
        if (st.locked) showLock();
      },
      () => showLock(),
    );
  }, [showLock]);
  // Ответ «locked» от любой команды — не сообщение, а экран входа.
  const setToast = useCallback((text: string | null) => (text === LOCKED ? goLocked() : showToast(text)), [goLocked]);

  // Сначала — вход: с PIN-кодом данные приходят только после него.
  useEffect(() => {
    api.lockStatus().then(
      (st) => {
        setLock(st);
        if (st.notice) showToast(st.notice);
        if (st.locked) showLock();
        else setReady(true);
      },
      () => setReady(true),
    );
    // Свернули — приложение считает время в фоне; вернулись через 5 минут — экран входа.
    const onVisible = () => {
      const visible = document.visibilityState === 'visible';
      api.appVisible(visible).then(
        (st) => {
          setLock(st);
          if (st.locked && visible) goLocked();
        },
        () => undefined,
      );
    };
    document.addEventListener('visibilitychange', onVisible);
    const unlisten = api.onLocked(goLocked).catch(() => () => undefined);
    return () => {
      document.removeEventListener('visibilitychange', onVisible);
      unlisten.then((f) => f());
    };
  }, [showLock, goLocked]);

  // Обмен по Wi-Fi и через облако идёт в фоне: пришло новое — перечитать.
  useEffect(() => {
    const lan = api.onLan((e) => (e.received > 0 || e.paired) && setSynced((n) => n + 1)).catch(() => () => undefined);
    const cloud = api.onCloud((e) => !e.running && e.received > 0 && setSynced((n) => n + 1)).catch(() => () => undefined);
    return () => {
      lan.then((f) => f());
      cloud.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => showToast(null), 4500);
    return () => window.clearTimeout(id);
  }, [toast]);

  const onUnlocked = useCallback((st: LockStatus) => {
    setLock(st);
    lockedRef.current = false;
    setLocked(false);
    setReady(true);
    if (st.recovery) setPinFlow('reset');
  }, []);

  const security = useMemo(
    () => ({
      lock,
      onPin: (m: PinMode) => setPinFlow(m),
      onBio: (on: boolean) => {
        api
          .bioSet(on)
          .then((st) => {
            setLock(st);
            setToast(st.error ?? (on ? 'Вход по отпечатку включён' : 'Вход по отпечатку выключен'));
          })
          .catch((e) => setToast(message(e)));
      },
      onLockNow: () => {
        api
          .lockNow()
          .then((st) => {
            setLock(st);
            if (st.locked) showLock();
          })
          .catch((e) => setToast(message(e)));
      },
    }),
    [lock, setToast, showLock],
  );

  const nav: Nav = useMemo(
    () => ({
      go: (r) => {
        setHistory([...history.slice(-20), route]);
        setRoute(r);
        document.querySelector('.main')?.scrollTo(0, 0);
      },
      back: () => {
        setRoute(history[history.length - 1] ?? { name: 'inbox' });
        setHistory(history.slice(0, -1));
      },
      toast: setToast,
    }),
    [route, history, setToast],
  );

  const goTab = (id: Route['name']) => {
    setHistory([]);
    setRoute({ name: id } as Route);
    document.querySelector('.main')?.scrollTo(0, 0);
  };

  /** Настройки, раздел «Ящики» — открытым. */
  const connect = () => {
    store('cut.accounts', '1');
    goTab('settings');
    window.setTimeout(() => document.getElementById('accounts')?.scrollIntoView({ block: 'start' }), 50);
  };
  // Новое письмо — на следующем этапе; пока ящиков нет, писать не с чего.
  const write = () => setToast('Сначала подключите ящик: Настройки → Ящики');
  const showFab = !wide && (route.name === 'inbox' || route.name === 'folders');

  let body;
  if (!ready) body = <p className="muted">Открываю…</p>;
  else if (route.name === 'settings')
    body = <Settings scheme={scheme} setScheme={setScheme} mode={mode} setMode={setMode} nav={nav} security={security} onSynced={() => setSynced((n) => n + 1)} />;
  else if (route.name === 'folders') body = <Folders />;
  else if (route.name === 'search') body = <Search />;
  else body = <Inbox synced={synced} onConnect={connect} />;

  return (
    <div className={wide ? 'app wide' : 'app phone'}>
      {wide && (
        <aside className="side" aria-label="Разделы">
          <p className="wordmark num">{NAME}</p>
          <button type="button" className="btn primary side-add" onClick={write}>
            <Icon name="pen" stroke={2} />
            <span>Написать</span>
          </button>
          {TABS.map((t) => (
            <button key={t.id} type="button" className={`navitem${route.name === t.id ? ' on' : ''}`} aria-current={route.name === t.id ? 'page' : undefined} onClick={() => goTab(t.id)}>
              <Icon name={t.icon} />
              <span>{t.label}</span>
            </button>
          ))}
        </aside>
      )}
      <main className="main">
        <div className={`inner${showFab ? ' with-fab' : ''}`}>{body}</div>
      </main>
      {showFab && (
        <button type="button" className="fab" aria-label="Написать письмо" onClick={write}>
          <Icon name="pen" size={24} stroke={2} />
        </button>
      )}
      {!wide && (
        <nav className="bottomnav" aria-label="Разделы">
          {TABS.map((t) => (
            <button key={t.id} type="button" className={route.name === t.id ? 'on' : ''} aria-current={route.name === t.id ? 'page' : undefined} onClick={() => goTab(t.id)}>
              <Icon name={t.icon} size={22} stroke={route.name === t.id ? 2 : 1.75} />
              <span>{t.label}</span>
            </button>
          ))}
        </nav>
      )}
      {locked && lock && <LockScreen key={lockKey} status={lock} api={api} names={{ name: NAME, nameAcc: NAME_ACC }} onUnlocked={onUnlocked} />}
      {pinFlow && !locked && (
        <PinFlow
          api={api}
          mode={pinFlow}
          onClose={() => setPinFlow(null)}
          onDone={(st, text) => {
            setLock(st);
            setPinFlow(null);
            showToast(text);
          }}
        />
      )}
      {toast && (
        <div className="toast" role="status" onClick={() => showToast(null)}>
          {toast}
        </div>
      )}
    </div>
  );
}
