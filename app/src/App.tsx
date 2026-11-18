import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { LockScreen, PinFlow, applyTheme, store, stored, type PinMode, type ThemeMode } from '@primus-fixer/kit/ui';
import { api, LOCKED, message, type LockStatus, type MailStatus } from './api';
import { Icon, NAME, NAME_ACC } from './ui';
import { FOLDERS, type FolderId } from './folders';
import Folder from './screens/Folder';
import Folders from './screens/Folders';
import Settings from './screens/Settings';

export type Route = { name: 'folder'; id: FolderId } | { name: 'folders' } | { name: 'settings' };

export type Nav = {
  go: (r: Route) => void;
  back: () => void;
  toast: (text: string) => void;
};

const INBOX: Route = { name: 'folder', id: 'inbox' };

/** Меню слева на компьютере: общие папки, свои папки ящиков, настройки. */
const MENU: { route: Route; label: string; icon: string }[] = [
  ...FOLDERS.map((f) => ({ route: { name: 'folder', id: f.id } as Route, label: f.label, icon: f.icon })),
  { route: { name: 'folders' }, label: 'Папки', icon: 'folder' },
  { route: { name: 'settings' }, label: 'Настройки', icon: 'settings' },
];

/** Нижняя панель на телефоне: «Входящие», «Папки» (там остальные папки), «Настройки». */
type Tab = 'inbox' | 'folders' | 'settings';
const TABS: { id: Tab; route: Route; label: string; icon: string }[] = [
  { id: 'inbox', route: INBOX, label: 'Входящие', icon: 'inbox' },
  { id: 'folders', route: { name: 'folders' }, label: 'Папки', icon: 'folder' },
  { id: 'settings', route: { name: 'settings' }, label: 'Настройки', icon: 'settings' },
];
const tabOf = (r: Route): Tab => (r.name === 'folder' ? (r.id === 'inbox' ? 'inbox' : 'folders') : r.name);
const same = (a: Route, b: Route) => a.name === b.name && (a.name !== 'folder' || (b.name === 'folder' && a.id === b.id));

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
  const [route, setRoute] = useState<Route>(INBOX);
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
  // Сколько ящиков подключено: от этого — что показывать в пустой папке.
  const [accounts, setAccounts] = useState(0);
  const [accountsRev, setAccountsRev] = useState(0);
  // Загрузка писем: состояние и счётчик её событий (по нему списки перечитываются), непрочитанные по папкам.
  const [mail, setMail] = useState<MailStatus | null>(null);
  const [mailRev, setMailRev] = useState(0);
  const [unread, setUnread] = useState<Record<string, number>>({});
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
          else if (visible) api.mailSyncNow().catch(() => undefined);
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
    if (!ready) return;
    api.accounts().then(
      (list) => setAccounts(list.length),
      () => undefined,
    );
  }, [ready, synced, accountsRev]);

  // Письма загружаются в фоне: события «mail» — ход загрузки и её конец.
  useEffect(() => {
    if (!ready) return;
    api.mailStatus().then(setMail, () => undefined);
    const un = api
      .onMail((st) => {
        setMail(st);
        setMailRev((n) => n + 1);
      })
      .catch(() => () => undefined);
    return () => {
      un.then((f) => f());
    };
  }, [ready]);

  useEffect(() => {
    if (!ready) return;
    api.unread().then(setUnread, () => undefined);
  }, [ready, synced, accountsRev, mailRev]);

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
    // Пока было заблокировано, письма не загружались.
    api.mailSyncNow().catch(() => undefined);
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
        setRoute(history[history.length - 1] ?? INBOX);
        setHistory(history.slice(0, -1));
      },
      toast: setToast,
    }),
    [route, history, setToast],
  );

  /** Раздел из меню или с нижней панели: история «Назад» начинается заново. */
  const goTop = (r: Route) => {
    setHistory([]);
    setRoute(r);
    document.querySelector('.main')?.scrollTo(0, 0);
  };

  /** Настройки, раздел «Ящики» — открытым. */
  const connect = () => {
    store('cut.accounts', '1');
    goTop({ name: 'settings' });
    window.setTimeout(() => document.getElementById('accounts')?.scrollIntoView({ block: 'start' }), 50);
  };
  // Новое письмо — на следующем этапе; пока ящиков нет, писать не с чего.
  const write = () => setToast(accounts ? 'Новое письмо появится вместе с загрузкой писем — в следующей версии' : 'Сначала подключите ящик: Настройки → Ящики');
  const showFab = !wide && route.name !== 'settings';

  let body;
  if (!ready) body = <p className="muted">Открываю…</p>;
  else if (route.name === 'settings')
    body = (
      <Settings scheme={scheme} setScheme={setScheme} mode={mode} setMode={setMode} nav={nav} security={security} onSynced={() => setSynced((n) => n + 1)} synced={synced} onAccounts={() => setAccountsRev((n) => n + 1)} />
    );
  else if (route.name === 'folders') body = <Folders wide={wide} onOpen={(id) => nav.go({ name: 'folder', id })} />;
  else {
    // На телефоне все папки, кроме «Входящих», открываются из «Папок» — у них сверху «Назад».
    const back = !wide && route.id !== 'inbox' ? () => (history.length ? nav.back() : goTop({ name: 'folders' })) : undefined;
    body = <Folder key={route.id} id={route.id} synced={synced} accounts={accounts} mail={mail} mailRev={mailRev} onConnect={connect} onBack={back} toast={setToast} />;
  }

  return (
    <div className={wide ? 'app wide' : 'app phone'}>
      {wide && (
        <aside className="side" aria-label="Разделы">
          <p className="wordmark num">{NAME}</p>
          <button type="button" className="btn primary side-add" onClick={write}>
            <Icon name="pen" stroke={2} />
            <span>Написать</span>
          </button>
          {MENU.map((m) => {
            const on = same(route, m.route);
            // Непрочитанные — у «Входящих» и «Спама».
            const role = m.route.name === 'folder' ? FOLDERS.find((f) => f.id === (m.route as { id: FolderId }).id)?.role : undefined;
            const count = role === 'inbox' || role === 'junk' ? unread[role] ?? 0 : 0;
            return (
              <button key={m.label} type="button" className={`navitem${on ? ' on' : ''}`} aria-current={on ? 'page' : undefined} onClick={() => goTop(m.route)}>
                <Icon name={m.icon} />
                <span>{m.label}</span>
                {count > 0 && <span className="navcount num">{count > 999 ? '999+' : count}</span>}
              </button>
            );
          })}
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
          {TABS.map((t) => {
            const on = tabOf(route) === t.id;
            return (
              <button key={t.id} type="button" className={on ? 'on' : ''} aria-current={on ? 'page' : undefined} onClick={() => goTop(t.route)}>
                <Icon name={t.icon} size={22} stroke={on ? 2 : 1.75} />
                <span>{t.label}</span>
              </button>
            );
          })}
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
